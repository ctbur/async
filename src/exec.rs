use log::{error, info};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::{fs, mem, process, thread};
use threadpool;

use super::error::{Error, Result, ResultExt};
use super::{comm, error};
use super::{CmdOp, ServerOp, WaitOp};

pub fn start_server<P: AsRef<Path>>(socket_path: P, op: ServerOp) -> Result<libc::pid_t> {
    let num_threads = op.num_threads.unwrap_or_else(num_cpus::get);
    let config = Config { num_threads };

    let pid = unsafe {
        fork(|| {
            let exec_handle = Executor::start(config);
            let res = comm::run_server(socket_path, exec_handle);
            error::report(res);
        })?
    };

    return Ok(pid);
}

struct Config {
    num_threads: usize,
}

enum ExecutorOp {
    Stop,
    Config(ServerOp),
    Cmd(CmdOp),
    Wait(WaitOp, Sender<TaskResults>),
    TaskRequest(TaskId, Sender<Result<process::Child>>),
    TaskReport(TaskId, TaskResult),
}

struct Executor {
    config: Config,
    threadpool: threadpool::ThreadPool,
    sender: Arc<Mutex<Sender<ExecutorOp>>>,
    receiver: Receiver<ExecutorOp>,
    work_plan: WorkPlan,
    waiting: Option<(WaitOp, Sender<TaskResults>)>,
}

impl Executor {
    fn start(config: Config) -> ExecutorHandle {
        let threadpool = threadpool::ThreadPool::new(config.num_threads);
        let (sender, receiver) = mpsc::channel();

        let exec = Self {
            config,
            threadpool,
            sender: Arc::new(Mutex::new(sender)),
            receiver,
            work_plan: WorkPlan::new(),
            waiting: None,
        };

        let sender_ret = exec.sender.clone();
        let handle = thread::spawn(move || exec.run());

        return ExecutorHandle {
            handle,
            sender: sender_ret,
        };
    }

    fn run(mut self) {
        let mut stop_requested = false;

        while !stop_requested {
            // TODO: remove unwrap
            let op = self.receiver.recv().unwrap();

            match op {
                ExecutorOp::Stop => {
                    // TODO: find way to kill running processes
                    stop_requested = true;
                }
                ExecutorOp::Cmd(op) => {
                    self.run_cmd(op);
                }
                ExecutorOp::Config(op) => {
                    self.reconfigure(op);
                }
                ExecutorOp::Wait(op, sender) => {
                    self.wait(op, sender);
                }
                ExecutorOp::TaskRequest(task_id, sender) => {
                    self.task_request(task_id, sender);
                }
                ExecutorOp::TaskReport(task_id, result) => {
                    self.task_report(task_id, result);
                }
            }
        }
    }

    fn run_cmd(&mut self, op: CmdOp) {
        let task_id = self.work_plan.enqueue_task(op);
        let exec_sender = self.sender.clone();

        self.threadpool
            .execute(move || execute_task(task_id, exec_sender));
    }

    fn reconfigure(&mut self, op: ServerOp) {
        if let Some(num_threads) = op.num_threads {
            info!(
                "Changing from {} to {} threads",
                self.config.num_threads, num_threads
            );
            self.config.num_threads = num_threads;
            self.threadpool.set_num_threads(num_threads);
        }
    }

    fn wait(&mut self, op: WaitOp, sender: Sender<TaskResults>) {
        // if work is already done, immediately send a response,
        //otherwise delay response until result of final task is reported
        if self.work_plan.is_done() {
            let results = self.work_plan.take_results();
            sender.send(results).unwrap();
        }
        if self.waiting.is_none() {
            self.waiting = Some((op, sender));
        } else {
            error!("Can only wait once at a time");
        }
    }

    fn task_request(&mut self, task_id: TaskId, sender: Sender<Result<process::Child>>) {
        info!("Task {} requested", task_id);

        let cmd = self.work_plan.view_task(task_id);
        let child_res = start_cmd(cmd);
        // TODO: remove unwrap
        sender.send(child_res).unwrap();
    }

    fn task_report(&mut self, task_id: TaskId, result: TaskResult) {
        info!("Task {} finished", task_id);
        self.work_plan.report_result(task_id, result);

        // if we are done and someone is waiting
        if self.work_plan.is_done() && self.waiting.is_some() {
            let results = self.work_plan.take_results();
            // send results and clear waiting
            let (_op, sender) = self.waiting.take().unwrap();
            sender.send(results).unwrap();
        }
    }
}

fn execute_task(task_id: TaskId, exec_sender: Arc<Mutex<Sender<ExecutorOp>>>) {
    let (sender, receiver) = mpsc::channel();

    // TODO: remove unwraps
    exec_sender
        .lock()
        .unwrap()
        .send(ExecutorOp::TaskRequest(task_id, sender))
        .unwrap();

    // TODO: remove unwrap
    let child_res = receiver.recv().unwrap();
    let exit_status = child_res.and_then(|mut child| Ok(child.wait()?));

    // TODO: remove unwraps
    exec_sender
        .lock()
        .unwrap()
        .send(ExecutorOp::TaskReport(task_id, exit_status))
        .unwrap();
}

pub struct ExecutorHandle {
    handle: thread::JoinHandle<()>,
    sender: Arc<Mutex<Sender<ExecutorOp>>>,
}

impl ExecutorHandle {
    pub fn run_cmd(&self, op: CmdOp) {
        // TODO: remove unwraps
        self.sender
            .lock()
            .unwrap()
            .send(ExecutorOp::Cmd(op))
            .unwrap();
    }

    pub fn reconfigure(&self, op: ServerOp) {
        // TODO: remove unwraps
        self.sender
            .lock()
            .unwrap()
            .send(ExecutorOp::Config(op))
            .unwrap();
    }

    pub fn wait(&self, op: WaitOp) -> TaskResults {
        let (sender, receiver) = mpsc::channel();

        // TODO: remove unwraps
        self.sender
            .lock()
            .unwrap()
            .send(ExecutorOp::Wait(op, sender))
            .unwrap();

        return receiver.recv().unwrap();
    }

    pub fn stop(self) {
        // TODO: remove unwraps
        self.sender.lock().unwrap().send(ExecutorOp::Stop).unwrap();
        self.handle.join().expect("execution thread panic");
    }
}

struct WorkPlan {
    task_id_counter: TaskId,
    queued_tasks: HashMap<TaskId, CmdOp>,
    finished_tasks: VecDeque<(CmdOp, TaskResult)>,
}

pub type TaskResult = Result<process::ExitStatus>;
pub type TaskResults = VecDeque<(CmdOp, TaskResult)>;
type TaskId = u64;

impl WorkPlan {
    fn new() -> Self {
        Self {
            task_id_counter: 0,
            queued_tasks: HashMap::new(),
            finished_tasks: TaskResults::new(),
        }
    }

    fn enqueue_task(&mut self, task: CmdOp) -> TaskId {
        let new_id = self.task_id_counter;
        self.task_id_counter += 1;

        self.queued_tasks.insert(new_id, task);
        return new_id;
    }

    fn view_task(&self, task_id: TaskId) -> &CmdOp {
        &self.queued_tasks.get(&task_id).unwrap()
    }

    fn report_result(&mut self, task_id: TaskId, res: TaskResult) {
        let cmd_op = self
            .queued_tasks
            .remove(&task_id)
            .expect("reporting results for inactive task");
        self.finished_tasks.push_back((cmd_op, res));
    }

    fn is_done(&self) -> bool {
        self.queued_tasks.is_empty()
    }

    fn take_results(&mut self) -> TaskResults {
        mem::replace(&mut self.finished_tasks, TaskResults::new())
    }
}

fn start_cmd(op: &CmdOp) -> Result<process::Child> {
    let (bin, args) = op.cmd[..].split_first().unwrap();

    // TODO: redirect stdout, stderr and write to stdin
    info!("Starting command: {}", op.cmd.join(" "));
    let mut cmd = process::Command::new(bin);
    cmd.args(args);

    // connect stdout, stderr and stdin if requested
    if let Some(ref out_path) = op.out_file {
        let out_file = fs::File::create(out_path)?;
        cmd.stdout(out_file);
    }
    if let Some(ref err_path) = op.err_file {
        let err_file = fs::File::create(err_path)?;
        cmd.stderr(err_file);
    }
    if let Some(ref in_path) = op.in_file {
        let in_file = fs::File::create(in_path)?;
        cmd.stdin(in_file);
    }

    let cmd_proc = cmd.spawn().with_context("Failed to start command")?;

    return Ok(cmd_proc);
}

unsafe fn fork<F: FnOnce()>(func: F) -> Result<libc::pid_t> {
    let pid = libc::fork();

    if pid < 0 {
        return Err(Error::with_context("failure during fork"));
    } else if pid == 0 {
        func();
        libc::exit(0);
    }

    return Ok(pid);
}
