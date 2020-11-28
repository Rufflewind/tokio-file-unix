extern crate tokio;
extern crate tokio_file_unix;

use tokio_util::codec::FramedRead;
use tokio_util::codec::LinesCodec;
use crate::tokio::stream::StreamExt;

#[actix_rt::main]
async fn main() {
    let file = tokio_file_unix::raw_stdin().unwrap();
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_io().unwrap();

    let mut framed = FramedRead::new(file, LinesCodec::new());

    println!("Type something and hit enter!");
    while let Some(got) = framed.next().await {
        println!("Got: {:?}", got);
    }
}