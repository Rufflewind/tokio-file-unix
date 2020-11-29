extern crate tokio;
extern crate tokio_file_unix;

use tokio::stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};

#[tokio::main]
async fn main() {
    // convert stdin into a nonblocking file;
    // this is the only part that makes use of tokio_file_unix
    let file = tokio_file_unix::raw_stdin().unwrap();
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_io().unwrap();

    let mut framed = FramedRead::new(file, LinesCodec::new());

    println!("Type something and hit enter!");
    while let Some(got) = framed.next().await {
        println!("Got: {:?}", got);
    }
}
