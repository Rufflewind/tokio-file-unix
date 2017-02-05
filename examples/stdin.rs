extern crate futures;
extern crate tokio_core;
extern crate tokio_file_unix;

fn main() {
    let mut core = tokio_core::reactor::Core::new().unwrap();
    let handle = core.handle();

    // convert stdin into a nonblocking file;
    // this is the only part that makes use of tokio_file_unix
    let file = std::fs::File::open("/dev/stdin").unwrap();
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_reader(&handle).unwrap();

    println!("Type something and hit enter!");
    use futures::{Future, future};
    core.run(future::loop_fn(file, |file| {
        // read each line
        tokio_core::io::read_until(file, b'\n', Vec::new()).map(|(file, line)| {

            // demonstrate that the event loop isn't blocked by I/O!
            let one_sec = std::time::Duration::new(1, 0);
            handle.spawn(
                tokio_core::reactor::Timeout::new(one_sec, &handle).unwrap()
                .map_err(|_| ())
                .map(|()| println!(" ... timeout works!"))
            );

            if line.ends_with(b"\n") {
                println!("Got: {:?}", std::str::from_utf8(&line));
                future::Loop::Continue((file))
            } else {                    // EOF
                future::Loop::Break(())
            }
        })
    })).unwrap();
}
