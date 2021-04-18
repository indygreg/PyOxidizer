# tugger-rust-toolchain

`tugger-rust-toolchain` is a library crate that facilitates discovering,
fetching, and using remote hosted Rust toolchains. It offers functionality
similar to what `rustup` does.

`tugger-rust-toolchain` is part of the Tugger application distribution tool
but exists as its own crate to facilitate code reuse for other tools
wishing to perform similar functionality. Tugger is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcome!
