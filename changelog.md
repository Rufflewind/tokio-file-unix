# Changelog

## 0.4.1

  - Documentation fixes.

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
