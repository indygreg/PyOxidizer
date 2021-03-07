# text-stub-library

`text-stub-library` is a library crate for reading and writing
*text stub files*, which define metadata about dynamic libraries.

*text stub files* are commonly materialized as `.tbd` files and
are commonly seen in Apple SDKs, where they serve as placeholders
for `.dylib` files, enabling linkers to work without access to
the full `.dylib` file.

This crate is developed as part of the PyOxidizer project and its
canonical home is https://github.com/indygreg/PyOxidizer. However,
the crate is intended to be useful on its own and modifications
to support its use outside of PyOxidizer are very much welcome!
