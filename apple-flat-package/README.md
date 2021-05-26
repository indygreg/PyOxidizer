# apple-flat-package

This crate implements an interface to Apple's *flat package* installer package
file format. This is the XAR-based installer package (`.pkg`) format used since
macOS 10.5.

The interface is in pure Rust and doesn't require the use of Apple specific
tools or hardware to run. The functionality in this crate could be used to
reimplement Apple installer tools like `pkgbuild` and `productbuild`.
