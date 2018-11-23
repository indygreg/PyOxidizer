# Change Log

## [Unreleased]

[Unreleased]: https://github.com/dgrunwald/rust-cpython/compare/0.2.1...HEAD

## 0.2.1 - 2018-09-28
- Added Python 3.7 support

## 0.2.0 - 2018-02-27
- Added `pub` modifier to `py_class!` syntax: `py_class!(pub class ClassName |py| ...)`
- Changed `obj.extract::<Vec<T>>(py)` to work with any object implementing the sequence protocol; not just lists.
- Added the `buffer` module, which allows safe access to the [buffer protocol](https://docs.python.org/3/c-api/buffer.html).
  This allows zero-copy access to numpy arrays.
- When building with `--feature nightly`, `extract::<Vec<PrimitiveType>>` will try to use the buffer protocol
  before falling back to the sequence protocol.
- [Added support for optional parameters][81] to `py_argparse!`, `py_fn!` and `py_class!` macros. (PR by [@Luthaf])

  Example: `py_fn!(py, function(i: i32 = 0))`
- Made `ObjectProtocol::compare()` available on Python 3.
- Added `ObjectProtocol::rich_compare()`.
- Fixed [non-deterministic segfault][115] in extension modules using `py_class!` (PR by [@markbt])
- Fixed python27-sys [compiler error on ARM][114] (PR by [@ostrosco])
- [Export path to Python interpreter as Cargo variable][119] (PR by [@indygreg])

[81]: https://github.com/dgrunwald/rust-cpython/pull/81
[@Luthaf]: https://github.com/Luthaf
[115]: https://github.com/dgrunwald/rust-cpython/pull/115
[@markbt]: https://github.com/markbt
[114]: https://github.com/dgrunwald/rust-cpython/pull/114
[@ostrosco]: https://github.com/ostrosco
[119]: https://github.com/dgrunwald/rust-cpython/pull/119
[@indygreg]: https://github.com/indygreg

## 0.1.0 - 2016-12-17
- First release that works on stable Rust.

