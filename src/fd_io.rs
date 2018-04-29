use std::os::unix::io::{AsRawFd, IntoRawFd};
use std::io;
use owned_fd::OwnedFd;

use nix;
use nix::unistd::{read, write};

pub struct FdIo {
    fd: OwnedFd,
}

impl FdIo {
    fn from<T: IntoRawFd>(fd: T) -> Self {
        FdIo {
            fd: OwnedFd::from(fd),
        }
    }
}

fn translate_nix_result<T>(res: nix::Result<T>) -> io::Result<T> {
    match res {
        Ok(size) => Ok(size),
        Err(nix::Error::Sys(e)) => Err(e.into()),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
    }
}

impl io::Read for FdIo {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        translate_nix_result(read(self.fd.as_raw_fd(), buf))
    }
}

impl io::Write for FdIo {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        translate_nix_result(write(self.fd.as_raw_fd(), buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
