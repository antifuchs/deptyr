use super::Selectable;

use std::os::unix::io::{AsRawFd, RawFd};
use std::io;

use nix;
use nix::unistd::{read, write};

/// helps convert errno-based errors into errors that io methods
/// understand.
fn translate_nix_result<T>(res: nix::Result<T>) -> io::Result<T> {
    match res {
        Ok(size) => Ok(size),
        Err(nix::Error::Sys(e)) => Err(e.into()),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
    }
}

#[derive(Copy, Clone)]
pub struct FdIo(RawFd);

impl FdIo {
    pub fn from<T>(i: T) -> FdIo
    where
        T: Sized + AsRawFd,
    {
        FdIo(i.as_raw_fd())
    }

    pub fn from_fd(i: RawFd) -> FdIo {
        FdIo(i)
    }
}

impl Selectable for FdIo {
    fn fd(&self) -> RawFd {
        self.as_raw_fd()
    }
}

impl io::Read for FdIo {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        translate_nix_result(read(self.0, buf))
    }
}

impl io::Write for FdIo {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        translate_nix_result(write(self.0, buf))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsRawFd for FdIo {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}
