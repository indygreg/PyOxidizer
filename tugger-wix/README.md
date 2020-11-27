# tugger-wix

`tugger-wix` is a library crate for interfacing with the
[WiX Toolset](https://wixtoolset.org/) - a set of tools for producing
Windows installers (e.g. `.msi` and `.exe` files).
The following functionality is (partially) implemented:

* A generic interface to invoke `candle.exe` and `light.exe` to process
  any set of input `.wxs` files.
* Automatic downloading and usage of the WiX Toolset.
* A builder interface for automatically constructing an `.msi` installer with
  common features - no WiX XML knowledge required!
* A builder interface for constructing an `.exe` bundle installer with
  common features - no WiX XML knowledge required!

`tugger-wix` is part of the Tugger application distribution tool
but exists as its own crate to facilitate code reuse for other tools
wishing to perform similar functionality. Tugger is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcome!
