# tugger-windows

`tugger-windows` is a library crate implementing functionality related
to packaging on Windows. The following functionality is (partially)
implemented:

* Locating the Windows SDK.
* Signing binaries with `signtool.exe`.
* Interfacing with code signing certificates.
* Locating the Visual C++ Redistributable runtime files.
* Locating `vswhere.exe`.

`tugger-windows` is part of the Tugger application distribution tool
but exists as its own crate to facilitate code reuse for other tools
wishing to perform similar functionality. Tugger is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcome!
