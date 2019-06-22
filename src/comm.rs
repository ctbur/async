use log::{debug, info};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::os::unix::net;
use std::path::Path;

use super::error::{Error, Result};
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
    if op.start || op.stop {
        // try stop server by communication, otherwise kill
        // Problems: timeout, error reporting
        //let client = Client::connect(socket_path, Protocol::Cmd)?;
        //client.transmit(&op)?;
    }
    if op.start {
        let pid = exec::start_server(&socket_path, op)?;
        info!("Server running at PID {}", pid);
    } else {
        let conn = Connection::connect(&socket_path, Protocol::Config)?;
        conn.transmit(&op)?;
    }

    return Ok(());
}

fn execute_cmd_op<P: AsRef<Path>>(socket_path: P, op: CmdOp) -> Result<()> {
    let conn = Connection::connect(socket_path, Protocol::Cmd)?;
    conn.transmit(&op)?;
    Ok(())
}

fn execute_wait_op<P: AsRef<Path>>(socket_path: P, op: WaitOp) -> Result<()> {
    let conn = Connection::connect(socket_path, Protocol::Wait)?;
    conn.transmit(&op)?;
    // receive response
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
        let client = Self {
            connection: net::UnixStream::connect(socket_path)?,
        };
        client.transmit(&protocol)?;
        return Ok(client);
    }

    fn transmit<M: Serialize>(&self, msg: &M) -> Result<()> {
        bincode::serialize_into(&self.connection, &msg)?;
        return Ok(());
    }

    fn receive<M: DeserializeOwned>(&self) -> Result<M> {
        let msg = bincode::deserialize_from(&self.connection)?;
        return Ok(msg);
    }
}

pub fn run_server<P: AsRef<Path>>(socket_path: P, mut executor: exec::Executor) -> Result<()> {
    let server = Server::create(socket_path)?;

    loop {
        let mut conn = server.accept()?;

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
                executor.wait(op);
            }
            Protocol::Stop => {
                executor.stop();
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
        let socket = net::UnixListener::bind(socket_path)?;
        return Ok(Server { socket });
    }

    fn accept(&self) -> Result<Connection> {
        let (connection, addr) = self.socket.accept()?;
        debug!("Client connected from address {:?}", addr);
        return Ok(Connection { connection });
    }

    /*fn run(&mut self) -> Result<()> {
        let mut consec_errs = 0;
        while consec_errs < MAX_CONSEC_ERRS && !self.exit_requested {
            let res = self.handle_client();
            match res {
                Ok(_) => {
                    consec_errs = 0;
                    debug!("Successfully handled client");
                }
                Err(err) => {
                    consec_errs + 1;
                    error!("{}", err);
                }
            }
        }

        return if consec_errs >= MAX_CONSEC_ERRS {
            Err(Error::MaxConsecErrs())
        } else {
            Ok(())
        };
    }*/
}
