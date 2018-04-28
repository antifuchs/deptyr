extern crate clap;
#[macro_use]
extern crate failure;
extern crate nix;
extern crate sendfd;

use std::ffi::CString;
use std::os::unix::ffi::OsStringExt;
use std::ffi::OsString;
use clap::{App, Arg, SubCommand};
use nix::unistd::execvp;
use failure::Error;

fn main() {
    let matches =
        App::new("deptyr")
            .arg(Arg::with_name("socket").short("s").required(true).takes_value(true).value_name("PATH").help(
                "UNIX domain socket for communication between 'run' and 'interact' subcommands.",
            ))
            .subcommands(vec![
                SubCommand::with_name("run")
                    .about("Send a PTY to the 'head' and execute command headlessly")
                    .arg(Arg::with_name("command-and-args").required(true).multiple(true).last(true)),
                SubCommand::with_name("interact")
                    .about("Runs the 'head' TTY, displaying output and allowing interaction"),
            ])
            .get_matches();

    match matches.subcommand() {
        ("run", Some(run_m)) => {
            let command: Vec<OsString> = run_m
                .values_of_os("command-and-args")
                .unwrap()
                .map(|arg| arg.to_os_string())
                .collect();
            run(command).unwrap();
        }
        ("interact", None) => {
            interact().unwrap();
        }
        (command, _) => {
            panic!("Unexpected subcommand {:?}", command);
        }
    }
}

fn interact() -> Result<(), Error> {
    println!("Starting head for socket (TK)");
    // TODO: Start UNIX domain socket server

    // TODO: Receive PTY

    // TODO: interact
    Ok(())
}

fn run(command: Vec<OsString>) -> Result<(), Error> {
    // TODO: Send PTY through unix domain socket

    // TODO: Redirect IO to PTY

    println!("Running: {:?}", command);
    unsafe {
        let cstr_command: Vec<CString> = command
            .into_iter()
            .map(|arg| CString::from_vec_unchecked(arg.into_vec()))
            .collect();
        try!(execvp(&cstr_command[0], &cstr_command));
        bail!("continued after execvp - this should never be reached");
    }
}
