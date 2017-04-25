extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_file_unix;

use std::io::Write;
use futures::Stream;

fn main() {
    let mut core = tokio_core::reactor::Core::new().unwrap();
    let handle = core.handle();

    // convert stdin into a nonblocking file;
    // this is the only part that makes use of tokio_file_unix
    let stdin = std::io::stdin();
    let file = tokio_file_unix::StdFile(stdin.lock());
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = match file.into_reader(&handle) {
        Err(ref e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            writeln!(std::io::stderr(),
                     "Error: Regular files are not supported.").unwrap();
            std::process::exit(1);
        }
        x => x,
    }.unwrap();

    println!("Type something and hit enter!");
    let line_codec = tokio_file_unix::DelimCodec(tokio_file_unix::Newline);
    let framed_read = tokio_io::codec::FramedRead::new(file, line_codec);
    core.run(framed_read.for_each(|line| {
        println!("Got: {:?}", std::str::from_utf8(&line));
        Ok(())
    })).unwrap();
}
