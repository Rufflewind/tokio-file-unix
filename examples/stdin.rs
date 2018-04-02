extern crate futures;
extern crate tokio;
extern crate tokio_file_unix;
extern crate tokio_io;

use futures::{future, Future};

fn main() {
    // convert stdin into a nonblocking file;
    // this is the only part that makes use of tokio_file_unix
    let stdin = std::io::stdin();
    let file = tokio_file_unix::StdFile(stdin.lock());
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_reader();

    println!("Type something and hit enter!");
    tokio::executor::current_thread::block_on_all(
        future::loop_fn((file, Vec::new()), |(file, line)| {
            // read each line
            tokio_io::io::read_until(file, b'\n', line).map(|(file, mut line)| {
                if line.ends_with(b"\n") {
                    // demonstrate that the event loop isn't blocked by I/O!
                    tokio::executor::current_thread::spawn(futures::lazy(|| {
                        Ok(println!("I'm asynchronous"))
                    }));

                    println!("Got: {:?}", std::str::from_utf8(&line));
                    line.clear();
                    future::Loop::Continue((file, line))
                } else {
                    // EOF
                    if !line.is_empty() {
                        println!("Got: {:?}", std::str::from_utf8(&line));
                    }
                    future::Loop::Break(())
                }
            })
        }).map_err(|e| println!("Error in loop! {}", e)),
    ).unwrap();
}
