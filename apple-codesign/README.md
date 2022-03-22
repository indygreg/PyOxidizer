# apple-codesign

`apple-codesign` is a crate implementing functionality related to code signing
on Apple platforms. Where possible, functionality is implemented in pure Rust
and doesn't rely on `codesign` or other proprietary Apple tools.

See the crate documentation at https://docs.rs/apple-codesign/latest/apple_codesign/
for more.

# `rcodesign` CLI

This crate defines an `rcodesign` binary which provides a CLI interface to
some of the crate's capabilities. To install:

```bash
# From a Git checkout
$ cargo run --bin rcodesign -- --help
$ cargo install --bin rcodesign

# Remote install.
$ cargo install --git https://github.com/indygreg/PyOxidizer --branch main rcodesign
```

# Project Relationship

`apple-codesign` is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcome!
