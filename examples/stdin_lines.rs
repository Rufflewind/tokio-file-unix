extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_file_unix;

use futures::Stream;

fn main() {
    let mut core = tokio_core::reactor::Core::new().unwrap();
    let handle = core.handle();

    // convert stdin into a nonblocking file;
    // this is the only part that makes use of tokio_file_unix
    let stdin = std::io::stdin();
    let file = tokio_file_unix::StdFile(stdin.lock());
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_reader(&handle).unwrap();

    println!("Type something and hit enter!");
    core.run(tokio_io::io::lines(file).for_each(|line| {
        println!("Got: {:?}", line);
        Ok(())
    })).unwrap();
}
