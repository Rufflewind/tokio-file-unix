extern crate libc;
extern crate mio;
extern crate tokio_core;

use std::io;
use std::os::unix::io::AsRawFd;
use tokio_core::reactor::{Handle, PollEvented};

/// A thin wrapper that adds support for `mio::Evented`.  The wrapper is
/// intended to be used with file-like objects that support `AsRawFd`.
///
/// You should construct it using `File::new_nb` unless you're certain the
/// underlying file descriptor has been set to nonblocking mode.
///
/// Example: creating a `File` for standard input:
///
/// ```
/// # use tokio_file_unix::*;
/// # fn test() -> std::io::Result<File<std::fs::File>> {
/// let file = File::new_nb(std::fs::File::open("/dev/stdin")?)?;
/// # Ok(file)
/// # }
/// ```
#[derive(Debug)]
pub struct File<F>(pub F);

impl<F: AsRawFd> File<F> {
    /// Wraps an existing file and enables nonblocking mode.  This modifies
    /// the flags of the underlying file descriptor.
    pub fn new_nb(mut file: F) -> io::Result<Self> {
        set_nonblocking(&mut file, true)?;
        Ok(File(file))
    }

    /// Converts into a pollable object that supports `std::io::Read`,
    /// `std::io::Write`, and `tokio_core::io::Io`, suitable for the
    /// `tokio_core::io::*` functions.
    pub fn into_io(self, handle: &Handle) -> io::Result<PollEvented<Self>> {
        Ok(PollEvented::new(self, handle)?)
    }
}

impl<F: AsRawFd + io::Read> File<F> {
    /// Converts into a pollable object that supports `std::io::Read` and
    /// `std::io::ReadBuf`, suitable for the `tokio_core::io::read_*`
    /// functions.
    pub fn into_reader(self, handle: &Handle)
                       -> io::Result<io::BufReader<PollEvented<Self>>> {
        Ok(io::BufReader::new(self.into_io(handle)?))
    }
}

impl<F: AsRawFd> mio::Evented for File<F> {
    fn register(&self, poll: &mio::Poll, token: mio::Token,
                interest: mio::Ready, opts: mio::PollOpt)
                -> io::Result<()> {
        mio::unix::EventedFd(&self.0.as_raw_fd())
            .register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &mio::Poll, token: mio::Token,
                  interest: mio::Ready, opts: mio::PollOpt)
                  -> io::Result<()> {
        mio::unix::EventedFd(&self.0.as_raw_fd())
            .reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        mio::unix::EventedFd(&self.0.as_raw_fd())
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

/// Sets nonblocking mode of the underlying file descriptor to either on
/// (`true`) or off (`false`).
///
/// Implementation detail: uses `fcntl` to set `O_NONBLOCK`.
pub fn set_nonblocking<F: AsRawFd>(file: &mut F, nonblocking: bool)
                                   -> io::Result<()> {
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

/// A `Codec` that splits the stream into frames divided by a given delimiter
/// byte.  All frames except possibly the last one contains the delimiter byte
/// as the last element.
///
/// ```
/// extern crate futures;
/// extern crate tokio_core;
/// # extern crate tokio_file_unix;
///
/// use futures::Stream;
/// use tokio_core::io::Io;
/// # use tokio_file_unix::*;
/// #
/// # fn main() {
/// # fn test(file: File<std::fs::File>, handle: &tokio_core::reactor::Handle)
/// # -> std::io::Result<Box<Stream<Item=String, Error=std::io::Error>>> {
///
/// fn string_from_utf8(s: Vec<u8>) -> std::io::Result<String> {
///     let err = std::io::Error::from(std::io::ErrorKind::InvalidData);
///     String::from_utf8(s).map_err(|_| err)
/// }
///
/// // convert a file into a stream of lines
/// let io = file.into_io(&handle)?;
/// let line_stream = io.framed(DelimCodec(Newline)).and_then(string_from_utf8);
///
/// # Ok(line_stream.boxed())
/// # }
/// # }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct DelimCodec<D>(pub D);

impl<D: Into<u8> + Copy> tokio_core::io::Codec for DelimCodec<D> {
    type In = Vec<u8>;
    type Out = Vec<u8>;

    fn decode(&mut self, buf: &mut tokio_core::io::EasyBuf)
              -> io::Result<Option<Self::In>> {
        Ok(buf.as_ref().iter().position(|b| *b == self.0.into())
           .map(|n| buf.drain_to(n + 1).as_ref().to_vec()))
    }

    fn decode_eof(&mut self, buf: &mut tokio_core::io::EasyBuf)
                  -> io::Result<Self::In> {
        Ok(buf.split_off(0).as_ref().to_vec())
    }

    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> io::Result<()> {
        buf.extend(msg);
        buf.push(self.0.into());
        Ok(())
    }
}

/// Represents a newline.  Implements `From<Newline>`, suitable for `DelimCodec`.
#[derive(Debug, Clone, Copy)]
pub struct Newline;

impl From<Newline> for u8 {
    fn from(_: Newline) -> Self {
        b'\n'
    }
}
