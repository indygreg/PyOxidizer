# pyembed

`pyembed` is a Rust library crate facilitating the control of Python
interpreters within Rust applications. It is a glorified wrapper around
the `pyo3` crate (which provides a Rust interface to Python's C APIs).
Its main value proposition over using `pyo3` directly is that it provides
additional value-add features such as integration with the
`oxidized_importer` extension module for importing Python modules and
resources from memory.

`pyembed` is part of the PyOxidizer Project but it is usable by any
Rust project embedding Python.
