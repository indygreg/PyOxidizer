# x509-certificate

`x509-certificate` is a library crate for interfacing with X.509 certificates.
It supports the following:

* Parsing certificates from BER, DER, and PEM.
* Serializing certificates to BER, DER, and PEM.
* Defining common algorithm identifiers.
* Generating new certificates.
* Verifying signatures on certificates.
* And more.

**This crate has not undergone a security audit. It does not
employ many protections for malformed data when parsing certificates.
Use at your own risk. See additional notes in `src/lib.rs`.**

`x509-certificate` is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

## Developing

The root of the repository is a Cargo workspace and has a lot of members.
The dependency tree for the entire repo is massive and `cargo build` likely
will fail due to Python dependency weirdness.

For best results, `cd x509-certificate` and run commands there. Or
`cargo build -p x509-certificate`, `cargo test -p x509-certificate`, etc.

This crate is used throughout this repository. If you want to build/run
the workspace, try
`cargo build --workspace --exclude oxidized-importer --exclude pyembed`
to exclude the crates most often causing build troubles. The `pyoxidizer`
and `tugger` crates have an expensive test harness and dependency tree and
can also be excluded.

Generally, the following crates are sensitive to changes in this one:

* `cryptographic-message-syntax`
* `apple-codesign`
* `tugger-code-signing`
* `tugger-windows-codesign`

If build + tests pass on these crates, there's a good chance the entire
workspace is happy.
