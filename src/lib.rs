//! A utility library that adds asynchronous support to file-like objects on
//! Unix-like platforms.
//!
//! This crate is primarily intended for pipes and other files that support
//! nonblocking I/O.  Regular files do not support nonblocking I/O, so this
//! crate has no effect on them.
//!
//! See [`File`](struct.File.html) for an example of how a file can be made
//! suitable for asynchronous I/O.

use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{fs, io};
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncWrite};

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

/// Wraps file-like objects for asynchronous I/O.
///
/// Normally, you should use `File::new_nb` rather than `File::raw_new` unless
/// the underlying file descriptor has already been set to nonblocking mode.
/// Using a file descriptor that is not in nonblocking mode for asynchronous
/// I/O will lead to subtle and confusing bugs.
///
/// Wrapping regular files has no effect because they do not support
/// nonblocking mode.
///
/// The most common instantiation of this type is `File<std::fs::File>`, which
/// provides the following trait implementation:
///
/// ```ignore
/// impl AsyncRead + AsyncWrite for File<std::fs::File>;
/// ```
///
/// While `F` is wrapped by `File`, the underlying file descriptor must not be
/// invalidated.
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
    fd: Option<AsyncFd<RawFd>>,
}

impl<F: AsRawFd> File<F> {
    /// Wraps a file-like object into a pollable object that supports
    /// `tokio::io::AsyncRead` and `tokio::io::AsyncWrite`, and also *enables
    /// nonblocking mode* on the underlying file descriptor.
    pub fn new_nb(mut file: F) -> io::Result<Self> {
        set_nonblocking(&mut file, true)?;
        File::raw_new(file)
    }

    /// Raw constructor that **does not enable nonblocking mode** on the
    /// underlying file descriptor.  This constructor should only be used if
    /// you are certain that the underlying file descriptor is already in
    /// nonblocking mode.
    pub fn raw_new(mut file: F) -> io::Result<Self> {
        // The raw file descriptor is being snapshotted here.  This is
        // necessary because (1) Async::new drops the argument upon failure
        // and (2) AsyncFdReadyGuard::with_io blocks us from using
        // AsyncFd::get_mut.
        let fd = match AsyncFd::new(file.as_raw_fd()) {
            // this is a workaround for regular files, which are not supported
            // by epoll; they would instead cause EPERM upon registration
            Err(ref e) if e.raw_os_error() == Some(libc::EPERM) => {
                set_nonblocking(&mut file, false)?;
                None
            }
            Ok(fd) => Some(fd),
            Err(e) => return Err(e),
        };
        Ok(Self { file, fd })
    }
}

impl<F: AsRawFd> AsRawFd for File<F> {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

impl<F> AsRef<F> for File<F> {
    fn as_ref(&self) -> &F {
        &self.file
    }
}

impl<F> AsMut<F> for File<F> {
    fn as_mut(&mut self) -> &mut F {
        &mut self.file
    }
}

impl<F: io::Read + Unpin> AsyncRead for File<F> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut tokio::io::ReadBuf,
    ) -> Poll<io::Result<()>> {
        let Self {
            ref mut file,
            ref mut fd,
        } = Pin::into_inner(self);
        match fd {
            Some(fd) => fd.poll_read_ready(cx)?.map(|mut guard| {
                let n = guard.with_io(|| file.read(buf.initialize_unfilled()))?;
                buf.advance(n);
                Ok(())
            }),
            None => {
                let n = file.read(buf.initialize_unfilled())?;
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
        }
    }
}

impl<F: io::Write + Unpin> AsyncWrite for File<F> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        let Self {
            ref mut file,
            ref mut fd,
        } = Pin::into_inner(self);
        match fd {
            Some(ref mut fd) => fd
                .poll_write_ready(cx)?
                .map(|mut guard| Ok(guard.with_io(|| file.write(buf))?)),
            None => Poll::Ready(file.write(buf)),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context) -> Poll<io::Result<()>> {
        Poll::Ready(Pin::into_inner(self).file.flush())
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn test_nonblocking() -> io::Result<()> {
        let (sock, _) = UnixStream::pair()?;
        let mut fd = sock.as_raw_fd();
        set_nonblocking(&mut fd, false)?;
        assert!(!get_nonblocking(&fd)?);
        set_nonblocking(&mut fd, true)?;
        assert!(get_nonblocking(&fd)?);
        set_nonblocking(&mut fd, false)?;
        assert!(!get_nonblocking(&fd)?);
        Ok(())
    }
}
