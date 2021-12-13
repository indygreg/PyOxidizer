# debian-packaging

`debian-packaging` is a library crate implementing functionality related
to Debian packaging. The following functionality is (partially)
implemented:

* Parsing and serializing control files
* Parsing `Release` and `InRelease` files.
* Parsing `Packages` files.
* Fetching Debian repository files from an HTTP server.
* Writing changelog files.
* Reading and writing `.deb` files.
* Creating repositories.
* PGP signing and verification operations.

See the crate's documentation for more.

# Project Relationship

`debian-packaging` is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcomed
and encouraged!
