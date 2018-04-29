extern crate clap;
#[macro_use]
extern crate failure;
extern crate libc;
#[macro_use]
extern crate nix;
extern crate owned_fd;
extern crate sendfd;

pub(crate) mod fd_io;
mod tty;

use fd_io::FdIo;
use tty::TTY;

use std::ffi::CString;
use std::ffi::OsString;
use std::fs::remove_file;
use std::io::{stderr, stdin, stdout, Read, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;
use std::thread::sleep;

use clap::{App, Arg, SubCommand};

use failure::Error;

use nix::Error as NixError;
use nix::errno::Errno;
use nix::unistd::{close, execvp, getppid, setpgid, setsid, Pid, dup2};
use nix::fcntl::{open, OFlag};
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt};
use nix::sys::termios::{cfmakeraw, tcgetattr, tcsetattr, SetArg, Termios};
use nix::sys::select::{pselect, FdSet};
use nix::sys::signal::{sigaction, sigprocmask, SaFlags, SigAction, SigHandler, SigSet, SigmaskHow,
                       Signal};
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
        ("interact", _) => {
            interact(socket).unwrap();
        }
        (command, _) => {
            panic!("Unexpected subcommand {:?}", command);
        }
    }
}

fn interact(socket_path: &str) -> Result<(), Error> {
    let listener = UnixListener::bind(socket_path)?;
    while let Ok((stream, _)) = listener.accept() {
        let pty = stream.recvfd()?;
        let rawmode = RawTermios::setup()?;

        match interact_with_process(pty) {
            Ok(()) => {
                drop(stream);
                rawmode.restore()?;
            }
            Err(e) => {
                drop(stream);
                rawmode.restore()?;
                println!("Encountered error: {}", e);
            }
        }
    }

    remove_file(socket_path)?;
    Ok(())
}

struct RawTermios {
    fd: RawFd,
    saved: Termios,
}

impl RawTermios {
    fn setup() -> Result<RawTermios, Error> {
        let fd = stdin().as_raw_fd();
        let termios = tcgetattr(fd)?;
        let mut raw = termios.clone();

        cfmakeraw(&mut raw);
        tcsetattr(fd, SetArg::TCSANOW, &raw)?;
        Ok(RawTermios {
            fd: fd,
            saved: termios,
        })
    }

    fn restore(&self) -> Result<(), Error> {
        loop {
            match tcsetattr(self.fd, SetArg::TCSANOW, &self.saved) {
                Ok(()) => {
                    break;
                }
                Err(NixError::Sys(Errno::EINTR)) => {
                    continue;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        Ok(())
    }
}

pub(crate) trait Selectable {
    fn fd(&self) -> RawFd;

    fn add_to_set(&self, set: &mut FdSet) {
        set.insert(self.fd());
    }
}

static mut WINCH_HAPPENED: bool = false;

extern "C" fn handle_winch(_: libc::c_int) {
    unsafe { WINCH_HAPPENED = true };
}

fn interact_with_process(pty: RawFd) -> Result<(), Error> {
    let mut buffer = vec![0 as u8; 4096];
    let mut pty = FdIo::from_fd(pty);
    let mut tty = TTY::default();

    let mut normal_mask = SigSet::empty();
    normal_mask.add(Signal::SIGWINCH);
    let select_mask = SigSet::empty();

    // block WINCH while pselect isn't running:
    sigprocmask(SigmaskHow::SIG_BLOCK, Some(&normal_mask), None)?;

    // handle WINCH when it's unblocked (in pselect):
    let handler = SigAction::new(
        SigHandler::Handler(handle_winch),
        SaFlags::empty(),
        normal_mask,
    );
    unsafe { sigaction(Signal::SIGWINCH, &handler)? };

    tty.resize_pty(pty)?;
    loop {
        if unsafe { WINCH_HAPPENED } {
            unsafe { WINCH_HAPPENED = false };
            // TODO: eliminate the unsafe signal handler by doing this in the EINTR branch below.
            tty.resize_pty(pty)?;
        }
        let mut fd_set = FdSet::new();
        tty.add_to_set(&mut fd_set);
        pty.add_to_set(&mut fd_set);
        match pselect(None, &mut fd_set, None, None, None, &select_mask) {
            Ok(_) => {}
            Err(NixError::Sys(Errno::EINTR)) => {
                continue;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
        if fd_set.contains(pty.fd()) {
            if proxy_write(&mut buffer, &mut pty, &mut tty)? {
                return Ok(());
            }
        }
        if fd_set.contains(tty.fd()) {
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
    tty::set_winsize(client_fd, tty::default_winsize())?;

    println!("Running: {:?}", command);
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
    let cstr_command: Vec<CString> = command
        .into_iter()
        .map(|arg| unsafe { CString::from_vec_unchecked(arg.into_vec()) })
        .collect();
    try!(execvp(&cstr_command[0], &cstr_command));
    bail!("continued after execvp - this should never be reached")
}
