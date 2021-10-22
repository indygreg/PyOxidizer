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
   Cargo.lock
   build.rs
   pyapp.exe.manifest
   pyapp-manifest.rc
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
the smarts for running an embedded Python interpreter.

In addition, the ``build = "build.rs"`` helps to dynamically configure the
crate.

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

Crate Features
==============

The auto-generated Rust project defines a number of features to control
behavior. These are documented in the sections below.

``build-mode-standalone``
-------------------------

This is the default build mode. It is enabled by default.

This build mode uses default Python linking behavior and feature detection
as implemented by the ``pyo3``. It will attempt to find a ``python`` in
``PATH`` or from the ``PYO3_PYTHON`` environment variable and link against it.

This is the default mode for convenience, as it enables the ``pyembed`` crate
to build in the most environments. However, the built binaries will have a
dependency against a foreign ``libpython`` and likely aren't suitable for
distribution.

This mode does not attempt to invoke ``pyoxidizer`` or find artifacts it would
have built. It is possible to build the ``pyembed`` crate in this mode if
the ``pyo3`` crate can find a Python interpreter. But, the ``pyembed``
crate may not be usable or work in the way you want it to.

This mode is intended to be used for performing quick testing on the
``pyembed`` crate. It is quite possible that linking errors will occur
in this mode unless you take additional actions to point Cargo at
appropriate libraries.

``pyembed`` has a dependency on Python 3.8+. If an older Python is detected,
it can result in build errors, including unresolved symbol errors.

``build-mode-pyoxidizer-exe``
-----------------------------

A ``pyoxidizer`` executable will be run to generate build artifacts.

The path to this executable can be defined via the ``PYOXIDIZER_EXE``
environment variable. Otherwise ``PATH`` will be used.

At build time, ``pyoxidizer run-build-script`` will be run. A
``PyOxidizer`` configuration file will be discovered using PyOxidizer's
heuristics for doing so. ``OUT_DIR`` will be set if running from ``cargo``,
so a ``pyoxidizer.bzl`` next to the main Rust project being built should
be found and used.

``pyoxidizer run-build-script`` will resolve the default build script target
by default. To override which target should be resolved, specify the target
name via the ``PYOXIDIZER_BUILD_TARGET`` environment variable. e.g.::

   $ PYOXIDIZER_BUILD_TARGET=build-artifacts cargo build

``build-mode-prebuilt-artifacts``
---------------------------------

This mode tells the build script to reuse artifacts that were already built.
(Perhaps you called ``pyoxidizer build`` or ``pyoxidizer run-build-script``
outside the context of a normal ``cargo build``.)

In this mode, the build script will look for artifacts in the directory
specified by ``PYOXIDIZER_ARTIFACT_DIR`` if set, falling back to ``OUT_DIR``.

``global-allocator-jemalloc``
-----------------------------

This feature will configure the Rust global allocator to use ``jemalloc``.

``global-allocator-mimalloc``
-----------------------------

This feature will configure the Rust global allocator to use ``mimalloc``.

``global-allocator-snmalloc``
-----------------------------

This feature will configure the Rust global allocator to use ``snmalloc``.

``allocator-jemalloc``
----------------------

This configures the ``pyembed`` crate with support for having the Python
interpreter use the ``jemalloc`` allocator.

``allocator-mimalloc``
----------------------

This configures the ``pyembed`` crate with support for having the Python
interpreter use the ``mimalloc`` allocator.

``allocator-snmalloc``
----------------------

This configures the ``pyembed`` crate with support for having the Python
interpreter use the ``snmalloc`` allocator.

Using Cargo With Generated Rust Projects
========================================

Building from a Rust project is not turn-key like PyOxidizer is.
PyOxidizer has to do some non-conventional things to get Rust projects to
build in very specific ways. Commands like ``pyoxidizer build`` abstract
away all of this complexity for you.

If you do want to use ``cargo`` directly, the following sections will give you
some tips.

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

An Example and Further Reference
==================================

Starting from a project freshly created with ``pyoxidizer init-rust-project sample``,
you'll first need to generate the build artifacts::

   $ PYOXIDIZER_EXECUTABLE=$HOME/.cargo/bin/pyoxidizer \
      PYO3_PYTHON=$HOME/python/install/bin/python3.9 \
      PYOXIDIZER_CONFIG=$(pwd)/pyoxidizer.bzl \
      TARGET=x86_64-apple-darwin \
      CARGO_MANIFEST_DIR=. \
      OUT_DIR=target/out \
      PROFILE=debug \
      pyoxidizer run-build-script build.rs

That will put the artifacts in target/out.

Then you can run cargo to build your crate::

   $ PYOXIDIZER_REUSE_ARTIFACTS=1 \
      PYOXIDIZER_ARTIFACT_DIR=$(pwd)/target/out \
      PYOXIDIZER_EXECUTABLE=$HOME/.cargo/bin/pyoxidizer \
      PYOXIDIZER_CONFIG=$(pwd)/pyoxidizer.bzl \
      PYO3_CONFIG_FILE=$(pwd)/target/out/pyo3-build-config-file.txt cargo \
      build --no-default-features --features \
         "build-mode-prebuilt-artifacts global-allocator-jemalloc allocator-jemalloc"

After building, you should find an executable in target/debug/.

Note that currently this does not produce any files that have been redirected to the filesystem,
such as extension modules. For now you'll need to copy them in from a normal pyoxidizer run, or
see https://github.com/indygreg/PyOxidizer/pull/466

On Windows, the paths will need updating, and the jemalloc features will need to be removed.

If you wish to dig further into how PyOxidizer builds projects, project_building.rs
is a good place to start.