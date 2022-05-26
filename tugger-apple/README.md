# tugger-apple

`tugger-apple` is a library crate implementing functionality related
to packaging on Apple. The following functionality is implemented:

* Mach-O universal binary creation
* Previous versions of this crate contained code for locating Apple SDKs.
  This code now lives as part of the [apple-sdk](https://crates.io/crates/apple-sdk)
  crate

`tugger-apple` is part of the Tugger application distribution tool
but exists as its own crate to facilitate code reuse for other tools
wishing to perform similar functionality. Tugger is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcome!
