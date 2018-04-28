extern crate clap;
#[macro_use]
extern crate failure;
extern crate libc;
#[macro_use]
extern crate nix;
extern crate sendfd;

use std::ffi::CString;
use std::ffi::OsString;
use std::io::{stderr, stdin, stdout};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;
use std::thread::sleep;

use clap::{App, Arg, SubCommand};

use failure::Error;

use libc::{TIOCGWINSZ, TIOCSWINSZ};

use nix::unistd::{close, execvp, getppid, setpgid, setsid, Pid, dup2};
use nix::fcntl::{open, OFlag};
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, Winsize};
// use nix::sys::termios::{cfmakeraw, Termios};
use nix::sys::stat::Mode;

use sendfd::UnixSendFd;

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

    let socket = matches.value_of("socket").unwrap();

    match matches.subcommand() {
        ("run", Some(run_m)) => {
            let command: Vec<OsString> = run_m
                .values_of_os("command-and-args")
                .unwrap()
                .map(|arg| arg.to_os_string())
                .collect();
            run(socket, command).unwrap();
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

ioctl!{bad write_ptr unsafe_tty_set_winsize with TIOCSWINSZ; Winsize}
ioctl!{bad read unsafe_tty_get_winsize with TIOCGWINSZ; Winsize}

fn default_winsize() -> Winsize {
    Winsize {
        ws_row: 80,
        ws_col: 30,
        ws_xpixel: 640,
        ws_ypixel: 480,
    }
}

fn tty_get_winsize(fd: RawFd) -> Result<Winsize, Error> {
    let mut ws = default_winsize();
    unsafe {
        try!(unsafe_tty_get_winsize(fd, &mut ws));
    }
    Ok(ws)
}

fn tty_set_winsize(fd: RawFd, ws: Winsize) -> Result<Winsize, Error> {
    unsafe {
        try!(unsafe_tty_set_winsize(fd, &ws));
    }
    Ok(ws)
}

fn resize_pty(fd: RawFd) -> Result<(), Error> {
    match tty_get_winsize(stdin().as_raw_fd()) {
        Err(_) => {
            // We have no tty on stdin - let's still tell the client
            // it has a "normal"-sized window:
            try!(tty_set_winsize(fd, default_winsize()));
        }
        Ok(ws) => {
            try!(tty_set_winsize(fd, ws));
        }
    }
    Ok(())
}

fn try_connect(socket_path: &str) -> UnixStream {
    loop {
        match UnixStream::connect(socket_path) {
            Ok(socket) => {
                return socket;
            }
            Err(e) => {
                println!(
                    "Could not connect to {}: {:?}. Retrying in 1s...",
                    socket_path, e
                );
                sleep(Duration::from_secs(1));
            }
        }
    }
}

fn run(socket_path: &str, command: Vec<OsString>) -> Result<(), Error> {
    // Open the socket:
    let socket = try_connect(socket_path);

    // Open the PTY:
    let controlling_fd = posix_openpt(OFlag::O_RDWR)?;
    grantpt(&controlling_fd)?;
    unlockpt(&controlling_fd)?;
    let client_pathname = unsafe { ptsname(&controlling_fd) }?; // POSIX calls this the "slave", but no.
    let client_fd = open(Path::new(&client_pathname), OFlag::O_RDWR, Mode::empty())?;
    tty_set_winsize(client_fd, default_winsize())?;

    // Make a new session & redirect IO to PTY
    setpgid(Pid::this(), getppid())?;
    setsid()?;
    let newstdin = open(Path::new(&client_pathname), OFlag::O_RDONLY, Mode::empty())?;
    dup2(newstdin, stdin().as_raw_fd())?;
    close(newstdin)?;

    let newout = open(Path::new(&client_pathname), OFlag::O_WRONLY, Mode::empty())?;
    dup2(newout, stdout().as_raw_fd())?;
    dup2(newout, stderr().as_raw_fd())?;
    close(newout)?;

    // send pty through the socket
    let pty_fd = controlling_fd.as_raw_fd();
    socket.sendfd(pty_fd)?;

    // Run the command:
    close(controlling_fd.as_raw_fd())?;
    println!("Running: {:?}", command);
    let cstr_command: Vec<CString> = command
        .into_iter()
        .map(|arg| unsafe { CString::from_vec_unchecked(arg.into_vec()) })
        .collect();
    try!(execvp(&cstr_command[0], &cstr_command));
    bail!("continued after execvp - this should never be reached")
}
