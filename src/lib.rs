//! A utility library that adds asynchronous support to file-like objects on
//! Unix-like platforms.
//!
//! See [`File`](struct.File.html) for an example of how a file can be made
//! suitable for asynchronous I/O.  See [`DelimCodec`](struct.DelimCodec.html)
//! for a more comprehensive example of reading the lines of a file using
//! `futures::Stream`.
extern crate bytes;
extern crate libc;
extern crate mio;
extern crate tokio_io;
extern crate tokio_core;

use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use bytes::{BufMut, BytesMut};
use tokio_io::codec;
use tokio_core::reactor::{Handle, PollEvented};

/// Wrapper for `std::io::Std*Lock` that can be used with `File`.
///
/// Note that the `Write` implementation for `StdinLock` always fails.
/// Similarly, the `Read` implementations for `StdoutLock` and `StderrLock`
/// always fail.  The extraneous implementations are needed to support
/// `tokio_io::AsyncRead::framed`.
///
/// For an example, see [`File`](struct.File.html).
///
/// ```
/// # /*
/// impl AsRawFd + Read + Write for File<StdinLock>
/// impl AsRawFd + Read + Write for File<StdoutLock>
/// impl AsRawFd + Read + Write for File<StderrLock>
/// # */
/// ```
pub struct StdFile<F>(pub F);

impl<'a> AsRawFd for StdFile<io::StdinLock<'a>> {
    fn as_raw_fd(&self) -> RawFd {
        libc::STDIN_FILENO
    }
}

impl<'a> AsRawFd for StdFile<io::StdoutLock<'a>> {
    fn as_raw_fd(&self) -> RawFd {
        libc::STDOUT_FILENO
    }
}

impl<'a> AsRawFd for StdFile<io::StderrLock<'a>> {
    fn as_raw_fd(&self) -> RawFd {
        libc::STDERR_FILENO
    }
}

impl<'a> io::Read for StdFile<io::StdinLock<'a>> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl<'a> io::Write for StdFile<io::StdinLock<'a>> {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::InvalidInput,
                           "cannot write to stdin"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::InvalidInput,
                           "cannot flush stdin"))
    }
}

impl<'a> io::Read for StdFile<io::StdoutLock<'a>> {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::InvalidInput,
                           "cannot read from stdout"))
    }
}

impl<'a> io::Write for StdFile<io::StdoutLock<'a>> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<'a> io::Read for StdFile<io::StderrLock<'a>> {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::InvalidInput,
                           "cannot read from stderr"))
    }
}

impl<'a> io::Write for StdFile<io::StderrLock<'a>> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Used to wrap file-like objects so they can be used with
/// `tokio_core::reactor::PollEvented`.
///
/// Normally, you should use `File::new_nb` rather than the `File` constructor
/// directly, unless the underlying file descriptor has already been set to
/// nonblocking mode.  Using a file that is not in nonblocking mode for
/// asynchronous I/O will lead to subtle bugs.
///
/// ```
/// # /*
/// impl Evented for File<std::fs::File>
/// impl Evented for File<StdFile<StdinLock>>
/// impl Evented for File<impl AsRawFd>
/// # */
/// ```
///
/// ## Example: wrapping standard input
///
/// ```
/// # use tokio_file_unix::*;
/// # fn test() -> std::io::Result<()> {
/// let stdin = std::io::stdin();
/// let file = File::new_nb(StdFile(stdin.lock()))?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct File<F>(pub F);

impl<F: AsRawFd> File<F> {
    /// Wraps a file-like object so it can be used with
    /// `tokio_core::reactor::PollEvented`, and also *enables nonblocking
    /// mode* on the underlying file descriptor.
    ///
    /// ```
    /// # /*
    /// fn(File<std::fs::File>, &Handle) -> Result<impl Evented + Read + Write>
    /// fn(File<StdFile<StdinLock>>, &Handle) -> Result<impl Evented + Read + Write>
    /// fn(File<impl AsRawFd>, &Handle) -> Result<impl Evented>
    /// # */
    /// ```
    pub fn new_nb(file: F) -> io::Result<Self> {
        let mut file = File(file);
        file.set_nonblocking(true)?;
        Ok(file)
    }

    /// Sets the nonblocking mode of the underlying file descriptor to either
    /// on (`true`) or off (`false`).  If `File::new_nb` was previously used
    /// to construct the `File`, then nonblocking mode has already been turned
    /// on.
    ///
    /// Implementation detail: uses `fcntl` to set `O_NONBLOCK`.
    pub fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()> {
        unsafe {
            let fd = self.as_raw_fd();
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

    /// Converts into a pollable object that supports `std::io::Read`,
    /// `std::io::Write`, `tokio_io::AsyncRead`, and `tokio_io::AsyncWrite`,
    /// making it suitable for `tokio_io::io::*`.
    ///
    /// ```
    /// # /*
    /// fn(File<std::fs::File>, &Handle) -> Result<impl Io>
    /// fn(File<StdFile<StdinLock>>, &Handle) -> Result<impl Io>
    /// fn(File<impl AsRawFd + Read>, &Handle) -> Result<impl Read>
    /// fn(File<impl AsRawFd + Write>, &Handle) -> Result<impl Write>
    /// fn(File<impl AsRawFd + Read + Write>, &Handle) -> Result<impl Io>
    /// # */
    /// ```
    pub fn into_io(self, handle: &Handle) -> io::Result<PollEvented<Self>> {
        Ok(PollEvented::new(self, handle)?)
    }
}

impl<F: AsRawFd + io::Read> File<F> {
    /// Converts into a pollable object that supports `std::io::Read` and
    /// `std::io::ReadBuf`, making it suitable for `tokio_io::io::read_*`.
    ///
    /// ```
    /// # /*
    /// fn(File<std::fs::File>, &Handle) -> Result<impl ReadBuf>
    /// fn(File<StdFile<StdinLock>>, &Handle) -> Result<impl ReadBuf>
    /// fn(File<impl AsRawFd + Read>, &Handle) -> Result<impl ReadBuf>
    /// # */
    /// ```
    pub fn into_reader(self, handle: &Handle)
                       -> io::Result<io::BufReader<PollEvented<Self>>> {
        Ok(io::BufReader::new(self.into_io(handle)?))
    }
}

impl<F: AsRawFd> AsRawFd for File<F> {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl<F: AsRawFd> mio::Evented for File<F> {
    fn register(&self, poll: &mio::Poll, token: mio::Token,
                interest: mio::Ready, opts: mio::PollOpt)
                -> io::Result<()> {
        mio::unix::EventedFd(&self.as_raw_fd())
            .register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &mio::Poll, token: mio::Token,
                  interest: mio::Ready, opts: mio::PollOpt)
                  -> io::Result<()> {
        mio::unix::EventedFd(&self.as_raw_fd())
            .reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        mio::unix::EventedFd(&self.as_raw_fd())
            .deregister(poll)
    }
}

impl<F: io::Read> io::Read for File<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl<F: io::Write> io::Write for File<F> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// A `Codec` that splits the stream into frames divided by a given delimiter
/// byte.  All frames except possibly the last one contain the delimiter byte
/// as the last element.
///
/// ```
/// # /*
/// impl Codec for DelimCodec<u8>
/// impl Codec for DelimCodec<Newline>
/// impl Codec for DelimCodec<impl Into<u8> + Copy>
/// # */
/// ```
///
/// ## Example: read stdin line by line
///
/// ```
/// extern crate futures;
/// extern crate tokio_io;
/// extern crate tokio_core;
/// # extern crate tokio_file_unix;
///
/// use futures::Stream;
/// use tokio_io::{AsyncRead, AsyncWrite};
/// # use tokio_file_unix::*;
/// #
/// # fn main() {
/// # fn test() -> std::io::Result<()> {
///
/// // initialize the event loop
/// let mut core = tokio_core::reactor::Core::new()?;
/// let handle = core.handle();
///
/// // get the standard input as a file
/// let stdin = std::io::stdin();
/// let io = File::new_nb(StdFile(stdin.lock()))?.into_io(&handle)?;
///
/// // turn it into a stream of lines, decoded as UTF-8
/// let line_stream = io.framed(DelimCodec(Newline)).and_then(|line| {
///     String::from_utf8(line).map_err(|_| {
///         std::io::Error::from(std::io::ErrorKind::InvalidData)
///     })
/// });
///
/// // specify how each line is to be processed
/// let future = line_stream.for_each(|line| {
///     println!("Got: {}", line);
///     Ok(())
/// });
///
/// // start the event loop
/// core.run(future)?;
///
/// # Ok(())
/// # }
/// # }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct DelimCodec<D>(pub D);

impl<D: Into<u8> + Copy> codec::Decoder for DelimCodec<D> {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<Self::Item>> {
        Ok(buf.as_ref().iter().position(|b| *b == self.0.into())
           .map(|n| buf.split_to(n + 1).as_ref().to_vec()))
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> io::Result<Option<Self::Item>> {
        let buf = buf.split_off(0);
        if buf.is_empty() {
            Ok(None)
        } else {
            Ok(Some(buf.as_ref().to_vec()))
        }
    }
}

impl<D: Into<u8> + Copy> codec::Encoder for DelimCodec<D> {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn encode(&mut self, msg: Self::Item, buf: &mut BytesMut) -> io::Result<()> {
        buf.extend(msg);
        buf.put_u8(self.0.into());
        Ok(())
    }
}

/// Represents a newline that can be used with `DelimCodec`.
///
/// For an example, see [`File`](struct.File.html).
///
/// ```
/// # /*
/// impl Into<u8> for Newline
/// # */
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Newline;

impl From<Newline> for u8 {
    fn from(_: Newline) -> Self {
        b'\n'
    }
}
