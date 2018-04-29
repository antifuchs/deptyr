use super::FdIo;
use super::Selectable;

use std::io;
use std::os::unix::io::{AsRawFd, RawFd};

use failure;
use libc::{TIOCGWINSZ, TIOCSWINSZ};
use nix::Error as NixError;
use nix::errno::Errno;
use nix::pty::Winsize;
use nix::sys::termios::{cfmakeraw, tcgetattr, tcsetattr, SetArg, Termios};

pub struct TTY {
    input: FdIo,
    output: FdIo,
    saved_mode: Option<Termios>,
}

impl Default for TTY {
    fn default() -> Self {
        TTY {
            input: FdIo::from(io::stdin()),
            output: FdIo::from(io::stdout()),
            saved_mode: None,
        }
    }
}

impl Selectable for TTY {
    fn fd(&self) -> RawFd {
        self.input.as_raw_fd()
    }
}

impl TTY {
    pub fn resize_pty<T>(&self, pty: T) -> Result<(), failure::Error>
    where
        T: AsRawFd + Sized,
    {
        let pty = pty.as_raw_fd();
        match get_winsize(self.input.as_raw_fd()) {
            Err(_) => {
                // We have no tty on stdin - let's still tell the client
                // it has a "normal"-sized window:
                try!(set_winsize(pty, default_winsize()));
            }
            Ok(ws) => {
                try!(set_winsize(pty, ws));
            }
        }
        Ok(())
    }

    pub fn setup_raw(&mut self) -> Result<(), failure::Error> {
        if self.saved_mode.is_some() {
            bail!("BUG in deptyr: Attempted to set up TTY more than once.");
        }
        let fd = self.input.as_raw_fd();
        let termios = tcgetattr(fd)?;
        let mut raw = termios.clone();

        cfmakeraw(&mut raw);
        tcsetattr(fd, SetArg::TCSANOW, &raw)?;
        self.saved_mode = Some(termios);
        Ok(())
    }
}

impl Drop for TTY {
    fn drop(&mut self) {
        if let Some(ref saved_mode) = self.saved_mode {
            loop {
                match tcsetattr(self.input.as_raw_fd(), SetArg::TCSANOW, &saved_mode) {
                    Ok(()) => {
                        break;
                    }
                    Err(NixError::Sys(Errno::EINTR)) => {
                        continue;
                    }
                    Err(e) => {
                        panic!("Could not restore terminal mode: {}", e);
                    }
                }
            }
        }
    }
}

impl io::Write for TTY {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Read for TTY {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.input.read(buf)
    }
}

ioctl_write_ptr_bad!{unsafe_tty_set_winsize, TIOCSWINSZ, Winsize}
ioctl_read_bad!{unsafe_tty_get_winsize, TIOCGWINSZ, Winsize}

pub fn default_winsize() -> Winsize {
    Winsize {
        ws_row: 80,
        ws_col: 30,
        ws_xpixel: 640,
        ws_ypixel: 480,
    }
}

fn get_winsize(fd: RawFd) -> Result<Winsize, failure::Error> {
    let mut ws = default_winsize();
    unsafe {
        try!(unsafe_tty_get_winsize(fd, &mut ws));
    }
    Ok(ws)
}

pub fn set_winsize(fd: RawFd, ws: Winsize) -> Result<Winsize, failure::Error> {
    unsafe {
        try!(unsafe_tty_set_winsize(fd, &ws));
    }
    Ok(ws)
}
