use log::info;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::{process, thread};
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
    Wait(WaitOp),
    TaskRequest(TaskId, mpsc::Sender<Result<process::Child>>),
    TaskReport(TaskId, TaskResult),
}

type ExecSender = mpsc::Sender<ExecutorOp>;
type ExecReceiver = mpsc::Receiver<ExecutorOp>;

struct Executor {
    config: Config,
    threadpool: threadpool::ThreadPool,
    sender: Arc<Mutex<ExecSender>>,
    receiver: ExecReceiver,
    work_plan: WorkPlan,
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
                ExecutorOp::Wait(op) => {
                    self.wait(op);
                }
                ExecutorOp::TaskRequest(task_id, sender) => {
                    let cmd = self.work_plan.view_task(task_id);
                    let child_res = start_cmd(cmd);
                    // TODO: remove unwrap
                    sender.send(child_res).unwrap();
                }
                ExecutorOp::TaskReport(task_id, result) => {
                    self.work_plan.report_result(task_id, result);
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
        // TODO
    }

    fn wait(&mut self, op: WaitOp) {
        // TODO
    }
}

fn execute_task(task_id: TaskId, exec_sender: Arc<Mutex<ExecSender>>) {
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
    sender: Arc<Mutex<ExecSender>>,
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

    pub fn wait(&self, op: WaitOp) {
        // TODO: remove unwraps
        self.sender
            .lock()
            .unwrap()
            .send(ExecutorOp::Wait(op))
            .unwrap();
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

type TaskId = u64;
type TaskResult = Result<process::ExitStatus>;

impl WorkPlan {
    fn new() -> Self {
        Self {
            task_id_counter: 0,
            queued_tasks: HashMap::new(),
            finished_tasks: VecDeque::new(),
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
}

fn start_cmd(op: &CmdOp) -> Result<process::Child> {
    let (bin, args) = op.cmd[..].split_first().unwrap();

    // TODO: redirect stdout, stderr and write to stdin
    info!("Starting command: {}", op.cmd.join(" "));
    let cmd_proc = process::Command::new(bin)
        .args(args)
        .spawn()
        .with_context("Failed to start command")?;

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
