# tokio-file-unix

Asynchronous support for `std::fs::File` via [Tokio](https://tokio.rs).  **Only supports Unix-like platforms.**

## Usage

Add this to your `Cargo.toml`:

~~~toml
[dependencies]
tokio-file-unix = "0.1.0"
~~~

Next, add this to the root module of your crate:

~~~rust
extern crate tokio_file_unix;
~~~

## Example

See the `examples` directory.

## License

Dual-licensed under Apache and MIT.
