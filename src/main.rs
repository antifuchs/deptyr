extern crate clap;
extern crate ctrlc;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate libc;
#[macro_use]
extern crate nix;
extern crate nix_ptsname_r_shim;
extern crate owned_fd;
extern crate sendfd;

pub(crate) mod fd_io;
mod tty;
mod unix_socket;

use fd_io::FdIo;
use tty::TTY;
use unix_socket::{send_control_pty, ListenError, ListenSocket};

use std::ffi::CString;
use std::ffi::OsString;
use std::io::{stderr, stdin, stdout, Read, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::{App, Arg, SubCommand};

use failure::{Error, ResultExt};

use nix::Error as NixError;
use nix::errno::Errno;
use nix::unistd::{close, execvp, getpgrp, setpgid, setsid, Pid, dup2};
use nix::fcntl::{open, OFlag};
use nix::pty::{grantpt, posix_openpt, unlockpt};
use nix::sys::select::{pselect, FdSet};
use nix::sys::signal::{sigaction, sigprocmask, SaFlags, SigAction, SigHandler, SigSet, SigmaskHow,
                       Signal};
use nix::sys::stat::Mode;

use nix_ptsname_r_shim::ptsname_r;

use owned_fd::OwnedFd;

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
        ("interact", _) => {
            interact(socket).unwrap();
        }
        (command, _) => {
            panic!("Unexpected subcommand {:?}", command);
        }
    }
}

/// Implemented for types wrapping raw FDs can be selected (for input).
pub(crate) trait InputSelectable {
    /// The input FD for this type.
    fn input_fd(&self) -> RawFd;

    /// Adds the input FD for this type to the `FdSet` for
    /// `select`ing.
    fn add_to_set(&self, set: &mut FdSet) {
        set.insert(self.input_fd());
    }

    /// True if the input FD is marked as readable in the `select`
    /// result.
    fn is_readable(&self, set: &mut FdSet) -> bool {
        set.contains(self.input_fd())
    }
}

fn interact(socket_path: &str) -> Result<(), Error> {
    let should_exit = Arc::new(AtomicBool::new(false));
    let r = should_exit.clone();
    ctrlc::set_handler(move || {
        r.store(true, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

    setup_sigwinch_handler()?;
    let listener =
        ListenSocket::listen(socket_path).context(format!("listening on {:?}", socket_path))?;

    while !should_exit.load(Ordering::SeqCst) {
        match listener.receive_pty() {
            Ok(pty) => {
                if let Err(e) = interact_with_process(pty) {
                    println!("Encountered error: {}", e);
                }
            }
            Err(ListenError::Canceled) => break,
            Err(e) => println!("Receiving PTY over UNIX domain socket: {}", e),
        }
    }
    Ok(())
}

/// A no-op, only intended to properly interrupt pselect
extern "C" fn handle_winch(_: libc::c_int) {}

/// Sets up a no-op handler for SIGWINCH and blocks the signal in
/// normal operation, so that `pselect` can get properly interrupted
/// whenever a window change occurs.
fn setup_sigwinch_handler() -> Result<(), Error> {
    let mut normal_mask = SigSet::empty();
    normal_mask.add(Signal::SIGWINCH);
    // block WINCH while pselect isn't running:
    sigprocmask(SigmaskHow::SIG_BLOCK, Some(&normal_mask), None)?;

    // handle WINCH when it's unblocked (in pselect):
    let handler = SigAction::new(
        SigHandler::Handler(handle_winch),
        SaFlags::empty(),
        normal_mask,
    );
    unsafe { sigaction(Signal::SIGWINCH, &handler)? };
    Ok(())
}

fn interact_with_process(pty_fd: OwnedFd) -> Result<(), Error> {
    let mut buffer = vec![0 as u8; 4096];
    let mut tty = TTY::default();
    let mut pty = FdIo::from_fd(pty_fd.as_raw_fd());

    tty.setup_raw()?;
    tty.resize_pty(pty)?;
    loop {
        let select_mask = SigSet::empty();
        let mut fd_set = FdSet::new();
        tty.add_to_set(&mut fd_set);
        pty.add_to_set(&mut fd_set);
        match pselect(None, &mut fd_set, None, None, None, &select_mask) {
            Ok(_) => {}
            Err(NixError::Sys(Errno::EINTR)) => {
                // This is likely due to a SIGWINCH, so resize:
                tty.resize_pty(pty)?;
                continue;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
        if pty.is_readable(&mut fd_set) {
            if proxy_write(&mut buffer, &mut pty, &mut tty)? {
                return Ok(());
            }
        }
        if tty.is_readable(&mut fd_set) {
            if proxy_write(&mut buffer, &mut tty, &mut pty)? {
                return Ok(());
            }
        }
    }
}

fn proxy_write<'a, R, W>(buffer: &mut [u8], r: &'a mut R, w: &'a mut W) -> Result<bool, Error>
where
    R: Read + Sized,
    W: Write + Sized,
{
    let n_read = r.read(buffer)?;
    if n_read == 0 {
        return Ok(true);
    }
    w.write_all(&buffer[..n_read])?;
    Ok(false)
}

fn setup_pty(socket_path: &str) -> Result<(), Error> {
    // Open the PTY:
    let controlling_fd = posix_openpt(OFlag::O_RDWR).context("posix_openpt")?;
    grantpt(&controlling_fd).context("grantpt")?;
    unlockpt(&controlling_fd).context("unlockpt")?;
    let client_pathname = ptsname_r(&controlling_fd).context("ptsname")?; // POSIX calls this the "slave", but no.

    // Make a new session & redirect IO to PTY
    if getpgrp() != Pid::this() {
        setpgid(Pid::from_raw(0), Pid::parent()).context("setpgid")?;
        setsid().context("setsid")?;
    }

    let newstdin = open(Path::new(&client_pathname), OFlag::O_RDONLY, Mode::empty())?;
    dup2(newstdin, stdin().as_raw_fd())?;
    tty::set_winsize(stdin().as_raw_fd(), tty::default_winsize()).context("initial setwinsize")?;
    close(newstdin)?;

    let newout = open(Path::new(&client_pathname), OFlag::O_WRONLY, Mode::empty())?;
    dup2(newout, stdout().as_raw_fd())?;
    dup2(newout, stderr().as_raw_fd())?;
    close(newout)?;

    // send controlling end of the pty through the socket:
    send_control_pty(socket_path, controlling_fd)
        .context(format!("sending PTY to {:?}", socket_path))?;
    Ok(())
}

fn run(socket_path: &str, command: Vec<OsString>) -> Result<(), Error> {
    println!("Running: {:?}", command);
    setup_pty(socket_path)?;

    let cstr_command: Vec<CString> = command
        .into_iter()
        .map(|arg| unsafe { CString::from_vec_unchecked(arg.into_vec()) })
        .collect();
    try!(execvp(&cstr_command[0], &cstr_command).context(format!("executing {:?}", cstr_command)));
    bail!("continued after execvp - this should never be reached")
}
