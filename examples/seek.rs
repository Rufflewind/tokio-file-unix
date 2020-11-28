extern crate tokio;
extern crate tokio_file_unix;

use tokio::prelude::*;
use tokio::io::AsyncWriteExt;
use std::fs::File as StdFile;
use std::io::{Seek, SeekFrom};

#[tokio::main]
async fn main() {
    let file = StdFile::create("tests/seek.txt").unwrap();
    file.set_len(0x11).unwrap();
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let mut file = file.into_io().unwrap();

    file.write_all(b"aaaaAAAAaaaaAAAA\n").await.unwrap();
    file.get_mut().seek(SeekFrom::Start(8)).unwrap();
    file.write_all(&[b'b'; 8]).await.unwrap();
    file.get_mut().seek(SeekFrom::Start(2)).unwrap();
    file.write_all(&[b'c'; 4]).await.unwrap();

    ()
}