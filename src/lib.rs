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
extern crate tokio_core;
extern crate tokio_io;

use std::cell::RefCell;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use bytes::{BufMut, BytesMut};
use tokio_core::reactor::{Handle, PollEvented};

/// Wrapper for `std::io::Std*Lock` that can be used with `File`.
///
/// For an example, see [`File`](struct.File.html).
///
/// ```ignore
/// impl AsRawFd + Read for File<StdinLock>
/// impl AsRawFd + Write for File<StdoutLock>
/// impl AsRawFd + Write for File<StderrLock>
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

impl<'a> io::Write for StdFile<io::StdoutLock<'a>> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
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
/// ```ignore
/// impl Evented for File<std::fs::File>;
/// impl Evented for File<StdFile<StdinLock>>;
/// impl Evented for File<impl AsRawFd>;
/// ```
///
/// ## Example: read standard input line by line
///
/// ```
/// extern crate futures;
/// extern crate tokio_core;
/// extern crate tokio_io;
/// extern crate tokio_file_unix;
///
/// use futures::Stream;
/// use tokio_io::{io, AsyncRead, AsyncWrite};
/// use tokio_io::codec::FramedRead;
/// use tokio_file_unix::{File, StdFile};
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
/// let reader = File::new_nb(StdFile(stdin.lock()))?.into_reader(&handle)?;
///
/// // turn it into a stream of lines and process them
/// let future = io::lines(reader).for_each(|line| {
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
    /// fn new_nb(StdFile<StdinLock>) -> Result<impl Evented + Read>;
    /// fn new_nb(StdFile<StdoutLock>) -> Result<impl Evented + Write>;
    /// fn new_nb(StdFile<StderrLock>) -> Result<impl Evented + Write>;
    /// fn new_nb(impl AsRawFd) -> Result<impl Evented>;
    /// ```
    pub fn new_nb(file: F) -> io::Result<Self> {
        let file = File::raw_new(file);
        file.set_nonblocking(true)?;
        Ok(file)
    }

    /// Sets the nonblocking mode of the underlying file descriptor to either
    /// on (`true`) or off (`false`).  If `File::new_nb` was previously used
    /// to construct the `File`, then nonblocking mode has already been turned
    /// on.
    ///
    /// Implementation detail: uses `fcntl` to set `O_NONBLOCK`.
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
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

    /// Converts into a pollable object that supports `tokio_io::AsyncRead`
    /// and `tokio_io::AsyncWrite`, making it suitable for `tokio_io::io::*`.
    ///
    /// ```ignore
    /// fn into_io(File<std::fs::File>, &Handle) -> Result<impl AsyncRead + AsyncWrite>;
    /// fn into_io(File<StdFile<StdinLock>>, &Handle) -> Result<impl AsyncRead + AsyncWrite>;
    /// fn into_io(File<impl AsRawFd + Read>, &Handle) -> Result<impl AsyncRead>;
    /// fn into_io(File<impl AsRawFd + Write>, &Handle) -> Result<impl AsyncWrite>;
    /// ```
    pub fn into_io(self, handle: &Handle) -> io::Result<PollEvented<Self>> {
        Ok(PollEvented::new(self, handle)?)
    }
}

impl<F: AsRawFd + io::Read> File<F> {
    /// Converts into a pollable object that supports `tokio_io::AsyncRead`
    /// and `std::io::BufRead`, making it suitable for `tokio_io::io::read_*`.
    ///
    /// ```ignore
    /// fn into_reader(File<std::fs::File>, &Handle) -> Result<impl AsyncRead + BufRead>;
    /// fn into_reader(File<StdFile<StdinLock>>, &Handle) -> Result<impl AsyncRead + BufRead>;
    /// fn into_reader(File<impl AsRawFd + Read>, &Handle) -> Result<impl AsyncRead + BufRead>;
    /// ```
    pub fn into_reader(self, handle: &Handle)
                       -> io::Result<io::BufReader<PollEvented<Self>>> {
        Ok(io::BufReader::new(self.into_io(handle)?))
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
    fn register(&self, poll: &mio::Poll, token: mio::Token,
                interest: mio::Ready, opts: mio::PollOpt)
                -> io::Result<()> {
        match mio::unix::EventedFd(&self.as_raw_fd())
                  .register(poll, token, interest, opts) {
            // this is a workaround for regular files, which are not supported
            // by epoll; they would instead cause EPERM upon registration
            Err(ref e) if e.raw_os_error() == Some(libc::EPERM) => {
                self.set_nonblocking(false)?;
                // workaround: PollEvented/IoToken always starts off in the
                // "not ready" state so we have to use a real Evented object
                // to set its readiness state
                let (r, s) = mio::Registration::new2();
                r.register(poll, token, interest, opts)?;
                s.set_readiness(mio::Ready::readable() |
                                     mio::Ready::writable())?;
                *self.evented.borrow_mut() = Some(r);
                Ok(())
            }
            e => e,
        }
    }

    fn reregister(&self, poll: &mio::Poll, token: mio::Token,
                  interest: mio::Ready, opts: mio::PollOpt)
                  -> io::Result<()> {
        match &*self.evented.borrow() {
            &None => mio::unix::EventedFd(&self.as_raw_fd())
                             .reregister(poll, token, interest, opts),
            &Some(ref r) => r.reregister(poll, token, interest, opts),
        }
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        match &*self.evented.borrow() {
            &None => mio::unix::EventedFd(&self.as_raw_fd())
                             .deregister(poll),
            &Some(ref r) => mio::Evented::deregister(r, poll),
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

/// A `Codec` that splits the stream into frames divided by a given delimiter
/// byte.  All frames except possibly the last one contain the delimiter byte
/// as the last element (this behavior differs from `tokio_io::io::lines`).
///
/// ```ignore
/// impl Codec for DelimCodec<u8>;
/// impl Codec for DelimCodec<Newline>;
/// impl Codec for DelimCodec<impl Into<u8> + Clone>;
/// ```
///
/// ## Example: read stdin line by line
///
/// ```
/// extern crate futures;
/// extern crate tokio_core;
/// extern crate tokio_io;
/// extern crate tokio_file_unix;
///
/// use futures::Stream;
/// use tokio_io::{io, AsyncRead, AsyncWrite};
/// use tokio_io::codec::FramedRead;
/// use tokio_file_unix::{File, StdFile, DelimCodec, Newline};
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
/// let line_stream = FramedRead::new(io, DelimCodec(Newline)).and_then(|line| {
///     String::from_utf8(line).map_err(|_| {
///         std::io::Error::from(std::io::ErrorKind::InvalidData)
///     })
/// });
///
/// // turn it into a stream of lines and process them
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

impl<D: Into<u8> + Clone> tokio_io::codec::Decoder for DelimCodec<D> {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut)
              -> Result<Option<Self::Item>, Self::Error> {
        Ok(buf.as_ref().iter().position(|b| *b == self.0.clone().into())
           .map(|n| buf.split_to(n + 1).as_ref().to_vec()))
    }

    fn decode_eof(&mut self, buf: &mut BytesMut)
                  -> Result<Option<Self::Item>, Self::Error> {
        let buf = buf.split_off(0);
        if buf.is_empty() {
            Ok(None)
        } else {
            Ok(Some(buf.as_ref().to_vec()))
        }
    }
}

impl<D: Into<u8> + Clone> tokio_io::codec::Encoder for DelimCodec<D> {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn encode(&mut self, msg: Self::Item, buf: &mut BytesMut)
              -> Result<(), Self::Error> {
        buf.extend(msg);
        buf.put_u8(self.0.clone().into());
        Ok(())
    }
}

/// Represents a newline that can be used with `DelimCodec`.
///
/// For an example, see [`File`](struct.File.html).
///
/// ```ignore
/// impl Into<u8> for Newline;
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Newline;

impl From<Newline> for u8 {
    fn from(_: Newline) -> Self {
        b'\n'
    }
}
