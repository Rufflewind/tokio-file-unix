//! A utility library that adds asynchronous support to file-like objects on
//! Unix-like platforms.
//!
//! This crate is primarily intended for pipes and other files that support
//! nonblocking I/O.  Regular files do not support nonblocking I/O, so this
//! crate has no effect on them.
//!
//! See [`File`](struct.File.html) for an example of how a file can be made
//! suitable for asynchronous I/O.

use std::cell::RefCell;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::{fs, io};
use tokio::io::PollEvented;

unsafe fn dupe_file_from_fd(old_fd: RawFd) -> io::Result<fs::File> {
    let fd = libc::fcntl(old_fd, libc::F_DUPFD_CLOEXEC, 0);
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fs::File::from_raw_fd(fd))
}

/// Duplicate the standard input file.
///
/// Unlike `std::io::Stdin`, this file is not buffered.
pub fn raw_stdin() -> io::Result<fs::File> {
    unsafe { dupe_file_from_fd(libc::STDIN_FILENO) }
}

/// Duplicate the standard output file.
///
/// Unlike `std::io::Stdout`, this file is not buffered.
pub fn raw_stdout() -> io::Result<fs::File> {
    unsafe { dupe_file_from_fd(libc::STDOUT_FILENO) }
}

/// Duplicate the standard error file.
///
/// Unlike `std::io::Stderr`, this file is not buffered.
pub fn raw_stderr() -> io::Result<fs::File> {
    unsafe { dupe_file_from_fd(libc::STDERR_FILENO) }
}

/// Gets the nonblocking mode of the underlying file descriptor.
///
/// Implementation detail: uses `fcntl` to retrieve `O_NONBLOCK`.
pub fn get_nonblocking<F: AsRawFd>(file: &F) -> io::Result<bool> {
    unsafe {
        let flags = libc::fcntl(file.as_raw_fd(), libc::F_GETFL);
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(flags & libc::O_NONBLOCK != 0)
    }
}

/// Sets the nonblocking mode of the underlying file descriptor to either on
/// (`true`) or off (`false`).  If `File::new_nb` was previously used to
/// construct the `File`, then nonblocking mode has already been turned on.
///
/// This function is not atomic. It should only called if you have exclusive
/// control of the underlying file descriptor.
///
/// Implementation detail: uses `fcntl` to query the flags and set
/// `O_NONBLOCK`.
pub fn set_nonblocking<F: AsRawFd>(file: &mut F, nonblocking: bool) -> io::Result<()> {
    unsafe {
        let fd = file.as_raw_fd();
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

/// Used to wrap file-like objects so they can be used with
/// `tokio_core::reactor::PollEvented`.
///
/// Normally, you should use `File::new_nb` rather than `File::raw_new` unless
/// the underlying file descriptor has already been set to nonblocking mode.
/// Using a file descriptor that is not in nonblocking mode for asynchronous
/// I/O will lead to subtle and confusing bugs.
///
/// Wrapping regular files has no effect because they do not support
/// nonblocking mode.
///
/// ```ignore
/// impl Evented for File<std::fs::File>;
/// impl Evented for File<impl AsRawFd>;
/// ```
///
/// ## Example: read standard input line by line
///
/// ```
/// use tokio::stream::StreamExt;
/// use tokio_util::codec::FramedRead;
/// use tokio_util::codec::LinesCodec;
///
/// #[tokio::main]
/// async fn main() -> std::io::Result<()> {
///     // convert stdin into a nonblocking file;
///     // this is the only part that makes use of tokio_file_unix
///     let file = tokio_file_unix::raw_stdin()?;
///     let file = tokio_file_unix::File::new_nb(file)?;
///     let file = file.into_io()?;
///
///     let mut framed = FramedRead::new(file, LinesCodec::new());
///
///     while let Some(got) = framed.next().await {
///         println!("Got this: {:?}", got);
///     }
///
///     println!("Received None, lol");
///     Ok(())
/// }
/// ```
///
/// ## Example: unsafe creation from raw file descriptor
///
/// To unsafely create `File<F>` from a raw file descriptor `fd`, you can do
/// something like:
///
/// ```
/// # use std::os::unix::io::{AsRawFd, RawFd};
/// use std::os::unix::io::FromRawFd;
///
/// # unsafe fn test<F: AsRawFd + FromRawFd>(fd: RawFd) -> std::io::Result<()> {
/// let file = tokio_file_unix::File::new_nb(F::from_raw_fd(fd))?;
/// # Ok(())
/// # }
/// ```
///
/// which will enable nonblocking mode upon creation.  The choice of `F` is
/// critical: it determines the ownership semantics of the file descriptor.
/// For example, if you choose `F = std::fs::File`, the file descriptor will
/// be closed when the `File` is dropped.
#[derive(Debug)]
pub struct File<F> {
    file: F,
    evented: RefCell<Option<mio::Registration>>,
}

impl<F: AsRawFd> File<F> {
    /// Wraps a file-like object so it can be used with
    /// `tokio_core::reactor::PollEvented`, and also *enables nonblocking
    /// mode* on the underlying file descriptor.
    ///
    /// ```ignore
    /// fn new_nb(std::fs::File) -> Result<impl Evented + Read + Write>;
    /// fn new_nb(impl AsRawFd) -> Result<impl Evented>;
    /// ```
    pub fn new_nb(mut file: F) -> io::Result<Self> {
        set_nonblocking(&mut file, true)?;
        Ok(File::raw_new(file))
    }

    /// Converts into a pollable object that supports `tokio_io::AsyncRead`
    /// and `tokio_io::AsyncWrite`, making it suitable for `tokio_io::io::*`.
    ///
    /// ```ignore
    /// fn into_io(File<std::fs::File>, &Handle) -> Result<impl AsyncRead + AsyncWrite>;
    /// fn into_io(File<impl AsRawFd + Read>, &Handle) -> Result<impl AsyncRead>;
    /// fn into_io(File<impl AsRawFd + Write>, &Handle) -> Result<impl AsyncWrite>;
    /// ```
    pub fn into_io(self) -> io::Result<PollEvented<Self>> {
        PollEvented::new(self)
    }
}

impl<F> File<F> {
    /// Raw constructor that **does not enable nonblocking mode** on the
    /// underlying file descriptor.  This constructor should only be used if
    /// you are certain that the underlying file descriptor is already in
    /// nonblocking mode.
    pub fn raw_new(file: F) -> Self {
        File {
            file: file,
            evented: Default::default(),
        }
    }
}

impl<F: AsRawFd> AsRawFd for File<F> {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

impl<F: AsRawFd> mio::Evented for File<F> {
    fn register(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> io::Result<()> {
        match mio::unix::EventedFd(&self.as_raw_fd()).register(poll, token, interest, opts) {
            // this is a workaround for regular files, which are not supported
            // by epoll; they would instead cause EPERM upon registration
            Err(ref e) if e.raw_os_error() == Some(libc::EPERM) => {
                set_nonblocking(&mut self.as_raw_fd(), false)?;
                // workaround: PollEvented/IoToken always starts off in the
                // "not ready" state so we have to use a real Evented object
                // to set its readiness state
                let (r, s) = mio::Registration::new2();
                r.register(poll, token, interest, opts)?;
                s.set_readiness(mio::Ready::readable() | mio::Ready::writable())?;
                *self.evented.borrow_mut() = Some(r);
                Ok(())
            }
            e => e,
        }
    }

    fn reregister(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> io::Result<()> {
        match *self.evented.borrow() {
            None => mio::unix::EventedFd(&self.as_raw_fd()).reregister(poll, token, interest, opts),
            Some(ref r) => r.reregister(poll, token, interest, opts),
        }
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        match *self.evented.borrow() {
            None => mio::unix::EventedFd(&self.as_raw_fd()).deregister(poll),
            Some(ref r) => mio::Evented::deregister(r, poll),
        }
    }
}

impl<F: io::Read> io::Read for File<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl<F: io::Write> io::Write for File<F> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl<F: io::Seek> io::Seek for File<F> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::io::{AsRawFd, RawFd};
    use std::os::unix::net::UnixStream;

    pub struct RefAsRawFd<T>(pub T);
    impl<'a, T: AsRawFd> AsRawFd for RefAsRawFd<&'a T> {
        fn as_raw_fd(&self) -> RawFd {
            self.0.as_raw_fd()
        }
    }

    #[test]
    fn test_nonblocking() -> io::Result<()> {
        let (sock, _) = UnixStream::pair()?;
        {
            let mut file = File::new_nb(RefAsRawFd(&sock))?;
            assert!(get_nonblocking(&file)?);
            set_nonblocking(&mut file, false)?;
            assert!(!get_nonblocking(&file)?);
            set_nonblocking(&mut file, true)?;
            assert!(get_nonblocking(&file)?);
            set_nonblocking(&mut file, false)?;
            assert!(!get_nonblocking(&file)?);
        }
        {
            let file = File::raw_new(RefAsRawFd(&sock));
            assert!(!get_nonblocking(&file)?);
        }
        Ok(())
    }
}
