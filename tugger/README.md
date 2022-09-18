# Tugger

Tugger is a generic application packaging and distribution tool.

Tugger implements its functionality across a series of crates:

* `tugger-binary-analysis` - Analyze platform native binaries.
* `tugger-common` - Shared functionality.
* `tugger-rpm` - RPM packaging.
* `tugger-snapcraft` - Snapcraft packaging.
* `tugger-windows` - Common Windows functionality (like binary signing).
* `tugger-wix` - WiX Toolset
* `tugger` - High-level interface and Starlark dialect.

Tugger is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project. However,
Tugger is intended to be useful as a standalone project and is developed as
such. However, its canonical source repository is the aforementioned
PyOxidizer repository.

## Status

Tugger is still very alpha and rough around the edges. You probably don't
want to use this crate.
