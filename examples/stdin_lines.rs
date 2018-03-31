extern crate futures;
extern crate tokio;
extern crate tokio_file_unix;

use futures::{Future, Stream};

fn main() {
    // convert stdin into a nonblocking file;
    // this is the only part that makes use of tokio_file_unix
    let file = tokio_file_unix::raw_stdin().unwrap();
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_reader(&tokio::reactor::Handle::current()).unwrap();

    println!("Type something and hit enter!");
    tokio::run(tokio::io::lines(file).for_each(|line| {
        println!("Got: {:?}", line);
        Ok(())
    }).map_err(|e| panic!("{:?}", e)));
}
