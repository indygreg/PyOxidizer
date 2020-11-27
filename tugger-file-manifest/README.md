# tugger-file-manifest

This crate provides a storage-agnostic interface for representing a collection
of files. It allows you to build up lists of files, which are composed of a path
name and content + metadata. The content can be backed by a referenced file
or defined in memory.

`tugger-file-manifest` is part of the Tugger application distribution tool
but exists as its own crate to facilitate code reuse for other tools
wishing to perform similar functionality. Tugger is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcome!
