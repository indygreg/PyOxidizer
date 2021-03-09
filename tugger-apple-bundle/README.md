# tugger-apple-bundle

`tugger-apple-bundle` is a library crate implementing functionality related
to Apple *bundles*, a foundational primitive in macOS and iOS for encapsulating
code and resources. (The ``.app`` *directories* in ``/Applications`` are
*application bundles*, for example.)

`tugger-apple-bundle` is part of the Tugger application distribution tool
but exists as its own crate to facilitate code reuse for other tools
wishing to perform similar functionality. Tugger is part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer.git) project and
this crate is developed in that repository.

While this crate is developed as part of a larger project, modifications
to support its use outside of its primary use case are very much welcome!
