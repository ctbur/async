use log::error;
use std::collections::VecDeque;
use std::path::Path;
use std::process;

use super::comm;
use super::error::{Error, Result};
use super::{CmdOp, ServerOp, WaitOp};

pub fn start_server<P: AsRef<Path>>(socket_path: P, op: ServerOp) -> Result<libc::pid_t> {
    let num_threads = op.num_threads.unwrap_or_else(num_cpus::get);
    let config = Config { num_threads };

    let pid = unsafe {
        fork(|| {
            let executor = Executor {
                config,
                cmd_queue: VecDeque::new(),
                active_processes: Vec::new(),
            };
            let result = comm::run_server(socket_path, executor);
            // TODO: log result
        })?
    };

    return Ok(pid);
}

struct Config {
    num_threads: usize,
}

pub struct Executor {
    config: Config,
    cmd_queue: VecDeque<CmdOp>,
    active_processes: Vec<process::Child>,
}

impl Executor {
    pub fn run_cmd(&mut self, op: CmdOp) {
        if op.cmd.is_empty() {
            error!("Empty command received");
            return;
        }

        self.cmd_queue.push_back(op);
        self.dispatch_cmds();
    }

    fn dispatch_cmds(&mut self) {
        while self.active_processes.len() < self.config.num_threads {
            let op_opt = self.cmd_queue.pop_front();

            if let Some(op) = op_opt {
                let child = start_cmd(op).unwrap();
                self.active_processes.push(child);
            } else {
                break;
            }
        }
    }

    pub fn reconfigure(&mut self, op: ServerOp) {
        if let Some(num_threads) = op.num_threads {
            self.config.num_threads = num_threads;
        }
    }

    pub fn wait(&mut self, op: WaitOp) {
        // whenever a process ends, start a new one
    }

    pub fn stop(&mut self) {
        for child in &mut self.active_processes {
            // TODO: save exit status for query
            child.kill().unwrap();
        }
    }
}

fn start_cmd(op: CmdOp) -> Result<process::Child> {
    let (bin, args) = op.cmd[..].split_first().unwrap();

    // TODO: redirect stdout, stderr and write to stdin
    let cmd_proc = process::Command::new(bin).args(args).spawn();
    // TODO: error handling
    return Ok(cmd_proc.unwrap());
}

unsafe fn fork<F: FnOnce()>(func: F) -> Result<libc::pid_t> {
    let pid = libc::fork();

    if pid < 0 {
        // TODO: return error
    } else if pid == 0 {
        func();
        libc::exit(0);
    }

    return Ok(pid);
}
