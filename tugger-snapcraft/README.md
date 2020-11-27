# tugger-snapcraft

`tugger-snapcraft` is a library crate for interfacing with the
[Snapcraft](https://snapcraft.io/) for building and interfacing with
snap packages.

The following functionality is (partially) implemented:

* Structs representing `snapcraft.yaml` primitives.
* Builder interface for invoking the `snapcraft` tool.

`tugger-snapcraft` is part of the Tugger application distribution tool
but exists as its own crate to facilitate code reuse for other tools
wishing to perform similar functionality. Tugger is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcome!
