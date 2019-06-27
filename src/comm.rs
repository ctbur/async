use log::{debug, info};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::os::unix::net;
use std::path::Path;
use std::{fmt, fs};

use super::error::{Result, ResultExt};
use super::exec;
use super::{CmdOp, Operation, ServerOp, WaitOp};

pub fn execute_operation<P: AsRef<Path>>(socket_path: P, op: Operation) -> Result<()> {
    match op {
        Operation::Server { op } => execute_server_op(socket_path, op),
        Operation::Cmd { op } => execute_cmd_op(socket_path, op),
        Operation::Wait { op } => execute_wait_op(socket_path, op),
    }
}

fn execute_server_op<P: AsRef<Path>>(socket_path: P, op: ServerOp) -> Result<()> {
    let socket_path = socket_path.as_ref();

    if op.stop {
        // send stop command, fail silently
        info!("Sending stop to server");
        let _ = Connection::connect(socket_path, Protocol::Stop)?;
    }
    if op.start {
        // send stop command, fail silently
        info!("Trying to send stop command to existing server");
        let res = Connection::connect(socket_path, Protocol::Stop);
        match res {
            Ok(_) => info!("Stopped server"),
            Err(_) => info!("No server found or unable to stop"),
        }

        if socket_path.exists() {
            info!("Removing file at {}", socket_path.display());
            fs::remove_file(socket_path).with_context("Unable to remove socket")?;
        }

        let pid = exec::start_server(socket_path, op)?;
        info!("Server running at PID {}", pid);
    } else {
        info!("Sending configuration to server");
        let conn = Connection::connect(socket_path, Protocol::Config)?;
        conn.transmit(&op)?;
    }

    return Ok(());
}

fn execute_cmd_op<P: AsRef<Path>>(socket_path: P, op: CmdOp) -> Result<()> {
    info!("Sending command to server");
    let conn = Connection::connect(socket_path, Protocol::Cmd)?;
    conn.transmit(&op)?;
    Ok(())
}

fn execute_wait_op<P: AsRef<Path>>(socket_path: P, op: WaitOp) -> Result<()> {
    info!("Sending wait to server");
    let conn = Connection::connect(socket_path, Protocol::Wait)?;
    conn.transmit(&op)?;
    let results: TaskResultsSer = conn.receive()?;

    for (op, res) in results.into_iter() {
        if let Err(msg) = res {
            eprintln!("Command \"{}\" failed: {}", op.cmd.join(" "), msg);
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
enum Protocol {
    Stop,
    Config,
    Cmd,
    Wait,
}

struct Connection {
    connection: net::UnixStream,
}

impl Connection {
    fn connect<P: AsRef<Path>>(socket_path: P, protocol: Protocol) -> Result<Self> {
        info!("Opening socket at {}", socket_path.as_ref().display());
        let client = Self {
            connection: net::UnixStream::connect(socket_path)
                .with_context("Unable to open connection to server on socket")?,
        };
        client.transmit(&protocol)?;
        return Ok(client);
    }

    fn transmit<M: Serialize + fmt::Debug>(&self, msg: &M) -> Result<()> {
        debug!("Sending message: {:?}", msg);
        bincode::serialize_into(&self.connection, &msg).with_context("Socket write error")?;
        return Ok(());
    }

    fn receive<M: DeserializeOwned + fmt::Debug>(&self) -> Result<M> {
        let msg = bincode::deserialize_from(&self.connection).with_context("Socket read error")?;
        debug!("Received message: {:?}", msg);
        return Ok(msg);
    }
}

pub fn run_server<P: AsRef<Path>>(socket_path: P, executor: exec::ExecutorHandle) -> Result<()> {
    let server = Server::create(&socket_path)?;

    loop {
        let conn = server.accept()?;

        let protocol: Protocol = conn.receive()?;
        debug!("Using protocol {:?}", protocol);

        match protocol {
            Protocol::Cmd => {
                let op: CmdOp = conn.receive()?;
                executor.run_cmd(op);
            }
            Protocol::Config => {
                let op: ServerOp = conn.receive()?;
                executor.reconfigure(op);
            }
            Protocol::Wait => {
                let op: WaitOp = conn.receive()?;
                let results = executor.wait(op);
                conn.transmit(&to_task_results_ser(results))?;
            }
            Protocol::Stop => {
                executor.stop();
                // TODO: send message when stopped
                break;
            }
        }
    }

    return Ok(());
}

struct Server {
    socket: net::UnixListener,
}

impl Server {
    fn create<P: AsRef<Path>>(socket_path: P) -> Result<Server> {
        info!("Creating socket at {}", socket_path.as_ref().display());
        let socket =
            net::UnixListener::bind(&socket_path).with_context("Unable to create socket")?;
        return Ok(Server { socket });
    }

    fn accept(&self) -> Result<Connection> {
        let (connection, addr) = self
            .socket
            .accept()
            .with_context("Unable to open connection to client")?;
        debug!("Client connected from address {:?}", addr);
        return Ok(Connection { connection });
    }
}

type TaskResultsSer = VecDeque<(CmdOp, std::result::Result<Option<i32>, String>)>;

fn to_task_results_ser(results: exec::TaskResults) -> TaskResultsSer {
    results
        .into_iter()
        .map(|(op, result)| match result {
            Ok(exit_status) => (op, Ok(exit_status.code())),
            Err(err) => (op, Err(format!("{}", err))),
        })
        .collect()
}
