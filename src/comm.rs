use log::{debug, error};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::os::unix::net;
use std::path::{Path, PathBuf};

use super::error::{Error, Result};
use super::exec;
use super::{CmdOp, Operation, ServerOp, WaitOp};

const MAX_CONSEC_ERRS: i32 = 3;

pub fn execute_operation<P: AsRef<Path>>(socket_path: P, op: Operation) -> Result<()> {
    match op {
        Operation::Server { op } => execute_server_op(socket_path, op),
        Operation::Cmd { op } => execute_cmd_op(socket_path, op),
        Operation::Wait { op } => execute_wait_op(socket_path, op),
    }
}

fn execute_server_op<P: AsRef<Path>>(socket_path: P, op: ServerOp) -> Result<()> {
    if op.start || op.stop {
        // Problems: timeout, error reporting
        //let client = Client::connect(socket_path, Protocol::Cmd)?;
        //client.transmit(op)?;
    }
    if op.start {
        exec::start_server(&socket_path);
    }

    let client = Client::connect(&socket_path, Protocol::Config)?;
    client.transmit(op)?;

    return Ok(());
}

fn execute_cmd_op<P: AsRef<Path>>(socket_path: P, op: CmdOp) -> Result<()> {
    let client = Client::connect(socket_path, Protocol::Cmd)?;
    client.transmit(op)?;
    Ok(())
}

fn execute_wait_op<P: AsRef<Path>>(socket_path: P, op: WaitOp) -> Result<()> {
    let client = Client::connect(socket_path, Protocol::Wait)?;
    client.transmit(op)?;
    // receive response
    Ok(())
}

#[derive(Serialize, Deserialize)]
enum Protocol {
    Stop,
    Config,
    Cmd,
    Wait,
}

struct Client {
    connection: net::UnixStream,
}

impl Client {
    fn connect<P: AsRef<Path>>(socket_path: P, protocol: Protocol) -> Result<Self> {
        let client = Client {
            connection: net::UnixStream::connect(socket_path)?,
        };
        client.transmit(protocol);
        return Ok(client);
    }

    fn transmit<M: Serialize>(&self, msg: M) -> Result<()> {
        serde_json::to_writer(&self.connection, &msg)?;
        return Ok(());
    }

    fn receive<M: DeserializeOwned>(&self) -> Result<M> {
        let msg = serde_json::from_reader(&self.connection)?;
        return Ok(msg);
    }
}

pub struct Config {
    pub num_threads: u32,
}

pub fn run_server<P: AsRef<Path>>(socket_path: P, config: Config) -> Result<()> {
    let mut server = Server::create(socket_path, config)?;
    return server.run();
}

struct Server {
    socket: net::UnixListener,
    config: Config,
    exit_requested: bool,
}

impl Server {
    fn create<P: AsRef<Path>>(socket_path: P, config: Config) -> Result<Server> {
        let socket = net::UnixListener::bind(socket_path)?;
        Ok(Server {
            socket,
            config,
            exit_requested: false,
        })
    }

    fn run(&mut self) -> Result<()> {
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
    }

    fn handle_client(&mut self) -> Result<()> {
        let (conn, addr) = self.socket.accept()?;
        debug!("Client connected from address {:?}", addr);

        loop {
            //let msg_opt: Option<Msg> = serde_json::from_reader(&conn)?;
        }
    }
}
