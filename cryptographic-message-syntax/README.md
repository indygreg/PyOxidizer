# cryptographic-message-syntax

`cryptographic-message-syntax` is a pure Rust implementation of
Cryptographic Message Syntax (CMS) as defined by RFC 5652. Also included
is Time-Stamp Protocol (TSP) (RFC 3161) client support.

From a high level CMS defines a way to digitally sign and authenticate
arbitrary content.

CMS is used to power the digital signatures embedded within Mach-O binaries
on Apple platforms such as macOS and iOS. The primitives in this crate could
be used to sign and authenticate Mach-O binaries. (See the sister
`tugger-apple-codesign` crate in the Git repository for code that does
just this.)

This crate is developed as part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and is
developed in that repository. While this crate is developed as part of a
larger product, it is authored such that it can be used standalone and
modifications to supports its use outside of its original use case are
very much welcome!
