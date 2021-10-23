.. py:currentmodule:: starlark_pyoxidizer

.. _rust:

==============================
PyOxidizer for Rust Developers
==============================

PyOxidizer is implemented in Rust. Binaries built with PyOxidizer are
also built with Rust using standard Rust projects.

While the existence of Rust should be abstracted away from most users
(aside from the existence of the install dependency and build output),
a target audience of PyOxidizer is Rust developers who want to embed
Python in a Rust project or Python developers who want to leverage
more Rust in their Python applications.

Follow the links below to learn how PyOxidizer uses Rust and how Rust
can be leveraged to build more advanced applications embedding Python.

.. toctree::
   :maxdepth: 2

   pyoxidizer_rust_cargo_source_checkouts
   pyoxidizer_rust_generic_embedding
   pyoxidizer_rust_projects
   pyoxidizer_rust_rust_code
   pyoxidizer_rust_porting
