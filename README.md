# `tokio-file-unix`

[![Documentation](https://docs.rs/tokio-file-unix/badge.svg)](https://docs.rs/tokio-file-unix)
[![Crates.io](https://img.shields.io/crates/v/tokio-file-unix.svg)](https://crates.io/crates/tokio-file-unix)
[![Travis CI Build Status](https://travis-ci.org/Rufflewind/tokio-file-unix.svg?branch=master)](https://travis-ci.org/Rufflewind/tokio-file-unix)

Asynchronous support for file-like objects via [Tokio](https://tokio.rs).  **Only supports Unix-like platforms.**

This crate is primarily intended for pipes and other files that support nonblocking I/O.  Regular files do not support nonblocking I/O, so this crate has no effect on them.

## Usage

Add this to your `Cargo.toml`:

~~~toml
[dependencies]
tokio-file-unix = "0.5.1"
~~~

Next, add this to the root module of your crate:

~~~rust
extern crate tokio_file_unix;
~~~

## Examples

See the `examples` directory as well as the documentation.

## License

Dual-licensed under Apache and MIT.
