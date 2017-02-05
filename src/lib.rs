extern crate libc;
extern crate mio;
extern crate tokio_core;

use std::{fs, io, os};
use tokio_core::reactor::{Handle, PollEvented};

/// A thin wrapper over `std::fs::File` that adds support for `mio::Evented`.
/// You should construct it using `File::new_nb` unless you're certain the
/// underlying file descriptor has been set to nonblocking mode.
///
/// Example: creating a `File` for standard input:
///
/// ```
/// # fn test() -> std::io::Result<tokio_file_unix::File> {
/// tokio_file_unix::File::new_nb(std::fs::File::open("/dev/stdin")?)
/// # }
/// ```
#[derive(Debug)]
pub struct File(pub fs::File);

impl File {
    /// Wraps an existing `std::fs::File` and enables nonblocking mode.
    /// This modifies the flags of the underlying file descriptor.
    pub fn new_nb(mut file: fs::File) -> io::Result<Self> {
        set_file_nonblocking(&mut file, true)?;
        Ok(File(file))
    }

    /// Converts into a pollable object that supports `std::io::Read`,
    /// `std::io::Write`, and `tokio_core::io::Io`, suitable for the
    /// `tokio_core::io::*` functions.
    pub fn into_io(self, handle: &Handle) -> io::Result<PollEvented<Self>> {
        Ok(PollEvented::new(self, handle)?)
    }

    /// Converts into a pollable object that supports `std::io::Read` and
    /// `std::io::ReadBuf`, suitable for the `tokio_core::io::read_*`
    /// functions.
    pub fn into_reader(self, handle: &Handle)
                       -> io::Result<io::BufReader<PollEvented<Self>>> {
        Ok(io::BufReader::new(self.into_io(handle)?))
    }
}

impl mio::Evented for File {
    fn register(&self, poll: &mio::Poll, token: mio::Token,
                interest: mio::Ready, opts: mio::PollOpt)
                -> io::Result<()> {
        mio::unix::EventedFd(&os::unix::io::AsRawFd::as_raw_fd(&self.0))
            .register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &mio::Poll, token: mio::Token,
                  interest: mio::Ready, opts: mio::PollOpt)
                  -> io::Result<()> {
        mio::unix::EventedFd(&os::unix::io::AsRawFd::as_raw_fd(&self.0))
            .reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        mio::unix::EventedFd(&os::unix::io::AsRawFd::as_raw_fd(&self.0))
            .deregister(poll)
    }
}

impl io::Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Write for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Sets nonblocking mode of the underlying file descriptor to either on
/// (`true`) or off (`false`).
///
/// Implementation detail: uses `fcntl` to set `O_NONBLOCK`.
pub fn set_file_nonblocking(file: &mut fs::File, nonblocking: bool)
                            -> io::Result<()> {
    let fd = os::unix::io::AsRawFd::as_raw_fd(file);
    set_fd_nonblocking(fd, nonblocking)
}

fn set_fd_nonblocking(fd: os::unix::io::RawFd, nonblocking: bool)
                      -> io::Result<()> {
    unsafe {
        // shamelessly copied from libstd/sys/unix/fd.rs
        let previous = libc::fcntl(fd, libc::F_GETFL);
        if previous < 0 {
            return Err(io::Error::last_os_error());
        }
        let new = if nonblocking {
            previous | libc::O_NONBLOCK
        } else {
            previous & !libc::O_NONBLOCK
        };
        if libc::fcntl(fd, libc::F_SETFL, new) < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
