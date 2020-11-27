# tugger-debian

`tugger-debian` is a library crate implementing functionality related
to Debian packaging. The following functionality is (partially)
implemented:

* Parsing and serializing control files
* Writing changelog files
* Writing `.deb` files (deterministically)

`tugger-debian` is part of the Tugger application distribution tool
but exists as its own crate to facilitate code reuse for other tools
wishing to have a low-level interface to Debian packaging primitives.
Tugger is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcome!
