.. py:currentmodule:: starlark_pyoxidizer

.. _rust_projects:

========================
PyOxidizer Rust Projects
========================

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
   .cargo/config
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

Using Cargo With Generated Rust Projects
========================================

Rust developers will probably want to use `cargo` instead of `pyoxidizer` for
building auto-generated Rust projects. This is supported, but behavior can
be very finicky.

PyOxidizer has to do some non-conventional things to get Rust projects to
build in very specific ways. Commands like ``pyoxidizer build`` abstract
away all of this complexity for you.

If you do want to use ``cargo`` directly, the following sections will give you
some tips.

``build.rs`` Invokes ``pyoxidizer``
-----------------------------------

The ``build.rs`` of the ``pyembed`` crate dependency will invoke ``pyoxidizer``
to generate various artifacts needed by the ``pyembed`` crate.

By default, it uses the ``pyoxidizer`` in ``PATH``. If you want to point it
at an explicit executable (this is common when you run ``pyoxidizer`` from
Git source checkouts), set the ``PYOXIDIZER_EXE`` environment variable. e.g.::

    $ PYOXIDIZER_EXE=~/src/pyoxidizer/target/debug/pyoxidizer cargo build

You may want to look at the source code of ``pyembed``'s ``build.rs`` for
all the magic that is being done.

Linking Against the Python Interpreter
--------------------------------------

The ``pyembed`` crate and some of its dependencies need to invoke a Python
interpreter to configure the Python interpreter settings. By default, they
look for ``python``, ``python3.9``, ``pythonX.Y`` executables on ``PATH``.

You can forcefully set the Python interpreter to use by setting the
``PYO3_PYTHON`` environment variable to the path of a Python interpreter.
For best results, use one of the default Python interpreters that your build
of PyOxidizer would use. Run
``pyoxidizer python-distribution-extract --help`` to see how you can
download and extract one of these distributions with ease.

Cargo Configuration
-------------------

Linking a custom libpython into the final Rust binary can be finicky, especially
when statically linking on Windows.

The auto-generated ``.cargo/config`` file defines some custom compiler settings
to enable things to work. However, this only works for some configurations. The
file contains some commented out settings that may need to be set for some
configurations (e.g. the ``standalone_static`` Windows distributions). Please
consult this file if running into build errors when not building through
``pyoxidizer``.
