use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::os::unix::net;
use std::path::{Path, PathBuf};

use super::error::{Error, Result};

const MAX_CONSEC_ERRS: i32 = 3;

#[derive(Serialize, Deserialize)]
pub enum Msg {
    Cmd(CmdMsg),
    Config(ConfigMsg),
}

#[derive(Serialize, Deserialize)]
pub struct CmdMsg {
    args: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ConfigMsg {
    num_threads: Option<u32>,
    exit_requested: bool,
}

pub struct Config {
    pub num_threads: u32,
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
            let msg_opt: Option<Msg> = serde_json::from_reader(conn)?;

            if let Some(msg) = msg_opt {
                match msg {
                    Msg::Cmd(cmd) => {}
                    Msg::Config(config) => {}
                }
            } else {
                return Ok(());
            }
        }
    }
}

pub fn run_server<P: AsRef<Path>>(socket_path: P, config: Config) -> Result<()> {
    let mut server = Server::create(socket_path, config)?;
    return server.run();
}

pub fn run_client<P: AsRef<Path>>(socket_path: P, msg: Msg) -> Result<()> {
    let conn = net::UnixStream::connect(socket_path)?;
    serde_json::to_writer(conn, &msg);
    return Ok(());
}
