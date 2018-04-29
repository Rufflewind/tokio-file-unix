# Changelog

## 0.5.1

  - Add `impl<F: Seek> Seek for File<F>`.

## 0.5.0

  - Migrate from `tokio-core` to `tokio-reactor`.
  - Add `raw_std{in,out,err}` and deprecate `StdFile` in favor of those.

## 0.4.2

  - Add `File::get_nonblocking`.

## 0.4.1

  - Improved documentation and added another example `stdin_lines.rs`.

## 0.4.0

  - Added “support” for regular files (which never block anyway).
    https://github.com/Rufflewind/tokio-file-unix/issues/2
  - Constructor of `File` is now private.
    Use `File::new_nb` or `File::raw_new` instead.
  - `File` is no longer `Sync`.
  - `File::set_nonblocking` no longer requires `&mut self`, just `&self`.

## 0.3.0

  - Removed fake implementations of `Read` and `Write` for `StdFile`.
  - Upgraded to tokio-io.

## 0.2.0

  - Added `DelimCodec` and `StdFile`.
  - Generalized `File`.

## 0.1.0

  - Initial release.
