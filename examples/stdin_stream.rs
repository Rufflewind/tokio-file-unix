extern crate futures;
extern crate tokio;
extern crate tokio_file_unix;
extern crate tokio_io;

use futures::{Future, Stream};

fn main() {
    // convert stdin into a nonblocking file;
    // this is the only part that makes use of tokio_file_unix
    let stdin = std::io::stdin();
    let file = tokio_file_unix::StdFile(stdin.lock());
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_reader();

    println!("Type something and hit enter!");
    let line_codec = tokio_file_unix::DelimCodec(tokio_file_unix::Newline);
    let framed_read = tokio_io::codec::FramedRead::new(file, line_codec);

    tokio::executor::current_thread::block_on_all(
        framed_read
            .for_each(|line| {
                println!("Got: {:?}", std::str::from_utf8(&line));
                Ok(())
            })
            .map_err(|e| println!("Error reading! {}", e)),
    ).unwrap();
}
