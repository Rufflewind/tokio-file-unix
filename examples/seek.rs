extern crate futures;
extern crate tokio;
extern crate tokio_file_unix;

use std::fs::File as StdFile;
use std::io::{Seek, SeekFrom};

use futures::Future;

fn main() {
    let file = StdFile::create("tests/seek.txt").unwrap();
    file.set_len(0x11).unwrap();
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_io(&tokio::reactor::Handle::current()).unwrap();

    let (mut file, _) = tokio::io::write_all(file, b"aaaaAAAAaaaaAAAA\n").wait().unwrap();
    file.get_mut().seek(SeekFrom::Start(8)).unwrap();
    let (mut file, _) = tokio::io::write_all(file, [b'b'; 8]).wait().unwrap();
    file.get_mut().seek(SeekFrom::Start(2)).unwrap();
    tokio::io::write_all(file, [b'c'; 4]).wait().unwrap();
}
