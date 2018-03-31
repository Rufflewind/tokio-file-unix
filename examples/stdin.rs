extern crate futures;
extern crate tokio;
extern crate tokio_file_unix;

use futures::{Future, future};

fn main() {
    // convert stdin into a nonblocking file;
    // this is the only part that makes use of tokio_file_unix
    let file = tokio_file_unix::raw_stdin().unwrap();
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_reader(&tokio::reactor::Handle::current()).unwrap();

    println!("Type something and hit enter!");
    tokio::run(future::loop_fn((file, Vec::new()), |(file, line)| {
        // read each line
        tokio::io::read_until(file, b'\n', line).map(|(file, mut line)| {

            // demonstrate that the event loop isn't blocked by I/O!
            let one_sec_from_now =
                std::time::Instant::now()
                + std::time::Duration::new(1, 0);
            tokio::spawn(
                tokio::timer::Delay::new(one_sec_from_now)
                .map_err(|_| ())
                .map(|()| eprintln!(" ... timeout works!"))
            );

            if line.ends_with(b"\n") {
                println!("Got: {:?}", std::str::from_utf8(&line));
                line.clear();
                future::Loop::Continue((file, line))
            } else {                    // EOF
                if !line.is_empty() {
                    println!("Got: {:?}", std::str::from_utf8(&line));
                }
                future::Loop::Break(())
            }
        })
    }).map_err(|e| panic!("{:?}", e)));
}
