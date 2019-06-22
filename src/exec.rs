use std::path::Path;

use super::comm;
use super::error::{Error, Result};
use super::{CmdOp, ServerOp, WaitOp};

pub fn start_server<P: AsRef<Path>>(socket_path: P, op: ServerOp) -> Result<libc::pid_t> {
    let num_threads = op.num_threads.unwrap_or_else(|| num_cpus::get() as u32);
    let config = Config { num_threads };

    let pid = unsafe {
        fork(|| {
            let executor = Executor;
            let result = comm::run_server(socket_path, executor);
            // TODO: log result
        })?
    };

    return Ok(pid);
}

struct Config {
    num_threads: u32,
}

pub struct Executor;

impl Executor {
    pub fn run_cmd(&self, op: CmdOp) {}

    pub fn reconfigure(&self, op: ServerOp) {}

    pub fn wait(&self, op: WaitOp) {}

    pub fn stop(&self) {}
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
