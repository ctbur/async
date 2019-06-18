use clap::{App, Arg, SubCommand};
use log::warn;
use std::result;
use std::str::FromStr;

mod error;
use error::Result;

mod peer;

fn is_a<T: FromStr>(val: String) -> result::Result<(), String> {
    match val.parse::<T>() {
        Ok(_) => Ok(()),
        Err(_) => Err(format!("illegal argument format: {}", val)),
    }
}

fn parse_args<'a>() -> clap::ArgMatches<'a> {
    let socket_path = Arg::with_name("socket_path")
        .short("s")
        .long("socket")
        .takes_value(true)
        .required(true)
        .help("The socket used for communication");

    let num_threads = Arg::with_name("num_threads")
        .short("j")
        .takes_value(true)
        .validator(is_a::<u32>)
        .help("How many processes to run in parallel");

    let server = SubCommand::with_name("server")
        .arg(socket_path.clone())
        .arg(num_threads.clone())
        .help_short("Starts a server");

    let stop = Arg::with_name("stop_server")
        .short("s")
        .long("stop-server")
        .help("Stops the server");

    let config = SubCommand::with_name("config")
        .arg(socket_path.clone())
        .arg(num_threads)
        .arg(stop);

    let cmd = Arg::with_name("cmd")
        .multiple(true)
        .allow_hyphen_values(true)
        .last(true);

    return App::new("Parallel")
        .version("0.1")
        .author("Cyrill Burgener <cyrill.burgener@gmail.com>")
        .about("Run your commands in parallel.")
        .arg(socket_path)
        .arg(cmd)
        .subcommand(server)
        .subcommand(config)
        .get_matches();
}

enum Operation {
    Server {
        socket_path: String,
        config: peer::Config,
    },
    Client {
        socket_path: String,
        msg: peer::Msg,
    },
}

fn decode_args<'a>(matches: clap::ArgMatches<'a>) -> Option<Operation> {
    if let Some(matches) = matches.subcommand_matches("server") {
        let socket_path = matches.value_of("socket_path").unwrap().to_owned();
        let num_threads = matches
            .value_of("num_threads")
            .unwrap()
            .parse::<u32>()
            .unwrap();

        let config = peer::Config { num_threads };
        return Some(Operation::Server {
            socket_path,
            config,
        });
    } else if let Some(matches) = matches.subcommand_matches("config") {
        let socket_path = matches.value_of("socket_path").unwrap().to_owned();
        let num_threads = matches
            .value_of("num_threads")
            .map(|v| v.parse::<u32>().unwrap());
        let exit_requested = matches.is_present("stop_server");

        let msg = peer::Msg::Config(peer::ConfigMsg {
            num_threads,
            exit_requested,
        });
        return Some(Operation::Client { socket_path, msg });
    } else {
        let socket_path = matches.value_of("socket_path").unwrap().to_owned();
        let cmd_opt = matches.values_of("cmd");
        let args: Vec<String> = match cmd_opt {
            None => return None,
            Some(values) => values.map(|s| s.to_owned()).collect(),
        };

        if args.is_empty() {
            warn!("No command given");
            return None;
        } else {
            let msg = peer::Msg::Cmd(peer::CmdMsg { args });
            return Some(Operation::Client { socket_path, msg });
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let matches = parse_args();

    let op = match decode_args(matches) {
        Some(op) => op,
        None => return Ok(()),
    };

    return match op {
        Operation::Server {
            socket_path,
            config,
        } => peer::run_server(socket_path, config),
        Operation::Client { socket_path, msg } => peer::run_client(socket_path, msg),
    };
}
