.. _rust_projects:

=============
Rust Projects
=============

PyOxidizer uses Rust projects to build binaries embedding Python.

If you just have a standalone configuration file (such as when running
``pyoxidizer init-config-file``), a temporary Rust project will be
created as part of building binaries. That project will be built, its
build artifacts copied, and the temporary project will be deleted.

If you use ``pyoxidizer init-rust-project`` to initialize a
``PyOxidizer`` application, the Rust project exists side-by-side with
the ``PyOxidizer`` configuration file and can be modified like
any other Rust project.

.. _rust_project_layout:

Layout
======

Generated Rust projects all have a similar layout::

   $ find pyapp -type f | grep -v .git
   Cargo.toml
   build.rs
   pyoxidizer.bzl
   src/main.rs

The ``Cargo.toml`` file is the configuration file for the Rust project.
Read more in
`the official Cargo documentation <https://doc.rust-lang.org/cargo/reference/manifest.html>`_.
The magic lines in this file to enable PyOxidizer are the following::

   [package]
   build = "build.rs"

   [dependencies]
   pyembed = ...

These lines declare a dependency on the ``pyembed`` package, which holds
the smarts for embedding Python in a binary.

In addition, the ``build = "build.rs"`` tells runs a script that hooks up
the output of the ``pyembed`` crate with this project.

Next let's look at ``src/main.rs``. If you aren't familiar with Rust
projects, the ``src/main.rs`` file is the default location for the source
file implementing an executable. If we open that file, we see a
``fn main() {`` line, which declares the *main* function for our executable.
The file is relatively straightforward. We import some symbols from the
``pyembed`` crate. We then construct a config object, use that to construct
a Python interpreter, then we run the interpreter and pass its exit code
to ``exit()``. Succinctly, we instantiate and run an embedded Python
interpreter. That's our executable.

The ``pyoxidizer.bzl`` is our auto-generated
:ref:`PyOxidizer configuration file <config_files>`.
