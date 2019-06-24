use log::debug;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use structopt::StructOpt;

mod comm;
mod error;
mod exec;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "parallel",
    about = "Run your commands in parallel.",
    version = "0.1",
    author = "Cyrill Burgener <cyrill.burgener@gmail.com>",
    rename_all = "kebab-case"
)]
pub struct Opt {
    /// The socket used for communication
    #[structopt(short = "s", long = "socket", parse(from_os_str))]
    socket_path: PathBuf,
    #[structopt(subcommand)]
    op: Operation,
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum Operation {
    /// Configure the server
    Server {
        #[structopt(flatten)]
        op: ServerOp,
    },
    /// Submit a command to the server
    Cmd {
        #[structopt(flatten)]
        op: CmdOp,
    },
    /// Wait for the completion of several commands
    Wait {
        #[structopt(flatten)]
        op: WaitOp,
    },
}

#[derive(Debug, StructOpt, Serialize, Deserialize)]
#[structopt(rename_all = "kebab-case")]
pub struct CmdOp {
    /// The file path where stdout is written to
    #[structopt(short = "o", long = "out", parse(from_os_str))]
    out_file: Option<PathBuf>,
    /// The file path where stderr is written to
    #[structopt(short = "e", long = "err", parse(from_os_str))]
    err_file: Option<PathBuf>,
    /// The file path where stdin is read from
    #[structopt(short = "i", long = "in", parse(from_os_str))]
    in_file: Option<PathBuf>,
    /// The command to be run on the server
    #[structopt(
        short = "-",
        raw(allow_hyphen_values = "true", last = "true", required = "true")
    )]
    cmd: Vec<String>,
}

#[derive(Debug, StructOpt, Serialize, Deserialize)]
#[structopt(rename_all = "kebab-case")]
pub struct ServerOp {
    /// Start or restart a server
    #[structopt(long = "start")]
    start: bool,
    /// Stop the server
    #[structopt(long = "stop")]
    stop: bool,
    /// How many processes to run in parallel
    #[structopt(short = "j", long = "num-threads")]
    num_threads: Option<usize>,
}

#[derive(Debug, StructOpt, Serialize, Deserialize)]
#[structopt(rename_all = "kebab-case")]
pub struct WaitOp {}

fn main() {
    env_logger::init();
    let opt = Opt::from_args();
    debug!("CLI options: {:?}", opt);

    let res = comm::execute_operation(opt.socket_path, opt.op);
    error::report(res);
}
