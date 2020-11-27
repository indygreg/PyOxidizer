# tugger-binary-analysis

`tugger-binary-analysis` is a library crate implementing functionality for
analyzing platform native binaries. The following functionality is
(partially) implemented:

* Defines mappings of gcc and glibc versions to Linux distributions.
* Obtain shared library dependencies of a binary.
* Find unresolved symbols in ELF binaries.
* Analyze a binary for machine portability. 

`tugger-binary-analysis` is part of the Tugger application distribution tool
but exists as its own crate to facilitate code reuse for other tools
wishing to reuse its functionality. Tugger is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcome!
