extern crate futures;
extern crate tokio_io;
extern crate tokio_core;
extern crate tokio_file_unix;

use futures::Stream;
use tokio_io::AsyncRead;

fn main() {
    let mut core = tokio_core::reactor::Core::new().unwrap();
    let handle = core.handle();

    // convert stdin into a nonblocking file;
    // this is the only part that makes use of tokio_file_unix
    let stdin = std::io::stdin();
    let file = tokio_file_unix::StdFile(stdin.lock());
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_io(&handle).unwrap();

    println!("Type something and hit enter!");
    let line_codec = tokio_file_unix::DelimCodec(tokio_file_unix::Newline);
    core.run(file.framed(line_codec).for_each(|line| {
        println!("Got: {:?}", std::str::from_utf8(&line));
        Ok(())
    })).unwrap();
}
