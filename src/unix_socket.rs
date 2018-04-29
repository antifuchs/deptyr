use super::InputSelectable;

use std::io;
use std::fs::remove_file;
use std::os::unix::io::{AsRawFd, IntoRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread::sleep;
use std::time::Duration;

use failure;
use nix;
use nix::errno::Errno;
use nix::sys::select::{select, FdSet};
use nix::pty::PtyMaster;

use owned_fd::OwnedFd;

use sendfd::UnixSendFd;

#[derive(Fail, Debug)]
pub(crate) enum ListenError {
    #[fail(display = "Aborted by user")]
    Canceled,

    #[fail(display = "low-level IO error")]
    Nix(#[cause] nix::Error),

    #[fail(display = "IO error")]
    Io(#[cause] io::Error),
}

impl From<io::Error> for ListenError {
    fn from(err: io::Error) -> ListenError {
        ListenError::Io(err)
    }
}

pub(crate) struct ListenSocket<'a> {
    path: &'a str,
    listener: UnixListener,
}

struct ReceivedFd(RawFd);

impl IntoRawFd for ReceivedFd {
    fn into_raw_fd(self) -> RawFd {
        self.0
    }
}

fn receive_fd(stream: &UnixStream) -> Result<ReceivedFd, io::Error> {
    Ok(ReceivedFd(stream.recvfd()?))
}

impl<'a> Drop for ListenSocket<'a> {
    fn drop(&mut self) {
        drop(&self.listener);
        if let Err(e) = remove_file(self.path) {
            panic!("Couldn't remove socket path: {}", e);
        }
    }
}

impl<'a> InputSelectable for ListenSocket<'a> {
    fn input_fd(&self) -> RawFd {
        self.listener.as_raw_fd()
    }
}

impl<'a> ListenSocket<'a> {
    pub(crate) fn listen(path: &'a str) -> Result<Self, failure::Error> {
        Ok(ListenSocket {
            path: path,
            listener: UnixListener::bind(path)?,
        })
    }

    pub(crate) fn receive_pty(&self) -> Result<OwnedFd, ListenError> {
        let mut fd_set = FdSet::new();
        self.add_to_set(&mut fd_set);
        match select(None, &mut fd_set, None, None, None) {
            Ok(_) => {
                let (stream, _) = self.listener.accept()?;
                let pty_fd = receive_fd(&stream)?;
                drop(stream);
                let owned = OwnedFd::from(pty_fd);
                Ok(owned)
            }
            Err(nix::Error::Sys(Errno::EINTR)) => Err(ListenError::Canceled),
            Err(e) => Err(ListenError::Nix(e)),
        }
    }
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

pub(crate) fn send_control_pty(
    path: &str,
    controlling_fd: PtyMaster,
) -> Result<(), failure::Error> {
    let socket = try_connect(path);

    socket.sendfd(controlling_fd.as_raw_fd())?;
    Ok(())
}
