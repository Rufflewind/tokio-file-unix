extern crate futures;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_file_unix;

use futures::{Future, Stream};

fn main() {
    // convert stdin into a nonblocking file;
    let file = tokio_file_unix::raw_stdin().unwrap();
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_reader(&tokio::reactor::Handle::current()).unwrap();

    println!("Type something and hit enter!");
    let line_codec = tokio_file_unix::DelimCodec(tokio_file_unix::Newline);
    let framed_read = tokio_io::codec::FramedRead::new(file, line_codec);
    tokio::run(framed_read.for_each(|line| {
        println!("Got: {:?}", std::str::from_utf8(&line));
        Ok(())
    }).map_err(|e| panic!("{:?}", e)));
}
