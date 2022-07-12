# apple-codesign

`apple-codesign` is a crate implementing functionality related to code signing
on Apple platforms.

Nearly all functionality is implemented in pure Rust: the only 3rd party tool
dependency is [Apple Transporter](https://help.apple.com/itc/transporteruserguide/),
which is used for uploading files to Apple for notarization. (Apple makes this tool
available on macOS, Windows, and Linux.) Signing and stapling are implemented in Rust
and do not require any 3rd party tools.

We believe this crate provides the most comprehensive implementation of Apple
code signing outside the canonical Apple tools. We have support for the following
features:

* Signing Mach-O binaries (the executable file format on Apple operating systems).
* Signing, notarizing, and stapling directory bundles (e.g. `.app` directories).
* Signing, notarizing, and stapling XAR archives / `.pkg` installers.
* Signing, notarizing, and stapling DMG disk images.

What this all means is that you can sign, notarize, and release Apple software
from Linux and Windows without needing access to proprietary Apple software.

See the crate documentation at https://docs.rs/apple-codesign/latest/apple_codesign/
and the end-user documentation at
https://gregoryszorc.com/docs/apple-codesign/main/ for more.

# `rcodesign` CLI

This crate defines an `rcodesign` binary which provides a CLI interface to
some of the crate's capabilities. To install:

```bash
# From a Git checkout
$ cargo run --bin rcodesign -- --help
$ cargo install --bin rcodesign

# Remote install.
$ cargo install --git https://github.com/indygreg/PyOxidizer --branch main --bin rcodesign apple-codesign
```

# Project Relationship

`apple-codesign` is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcome!
