.. _overview:

========
Overview
========

From a very high level, ``PyOxidizer`` is a tool for packaging and
distributing Python applications. The over-arching goal of ``PyOxidizer``
is to make this (often complex) problem space simple so application
maintainers can focus on building quality applications instead of
toiling with build systems and packaging tools.

On a lower, more technical level, ``PyOxidizer`` has a command line
tool - ``pyoxidizer`` - that is capable of building binaries (executables
or libraries) that embed a fully-functional Python interpreter plus
Python extensions and modules *in a single binary*. Binaries produced
with ``PyOxidizer`` are highly portable and can work on nearly every
system without any special requirements like containers, FUSE filesystems,
or even temporary directory access. On Linux, ``PyOxidizer`` can
produce executables that are fully statically linked and don't even
support dynamic loading.

The *Oxidizer* part of the name comes from Rust: binaries built with
``PyOxidizer`` are compiled from Rust and Rust code is responsible for
managing the embedded Python interpreter and all its operations. But the
existence of Rust should be invisible to many users, much like the fact
that CPython (the official Python distribution available from www.python.org)
is implemented in C. Rust is simply a tool to achieve an end goal (albeit
a rather effective and powerful tool).

Components
==========

The most visible component of ``PyOxidizer`` is the ``pyoxidizer`` command
line tool. This tool contains functionality for creating new projects using
``PyOxidizer``, adding ``PyOxidizer`` to existing projects, producing
binaries containing a Python interpreter, and various related functionality.

The ``pyoxidizer`` executable is written in Rust. Behind that tool is a pile
of Rust code performing all the functionality exposed by the tool. That code
is conveniently also made available as a library, so anyone wanting to
integrate ``PyOxidizer``'s core functionality without using our ``pyoxidizer``
tool is able to do so.

The ``pyoxidizer`` crate and command line tool are effectively glorified build
tools: they simply help with various project management, build, and packaging.

The run-time component of ``PyOxidizer`` is completely separate from the
build-time component. The run-time component of ``PyOxidizer`` consists of a
Rust crate named ``pyembed``. The role of the ``pyembed`` crate is to manage an
embedded Python interpreter. This crate contains all the code needed to
interact with the CPython APIs to create and run a Python interpreter.
``pyembed`` also contains the special functionality required to import
Python modules from memory using zero-copy.

How It Works
============

The ``pyoxidizer`` tool is used to create a new project or add ``PyOxidizer``
to an existing (Rust) project. This entails:

* Adding a copy of the ``pyembed`` crate to the project.
* Generating a boilerplate Rust source file to call into the ``pyembed`` crate
  to run a Python interpreter.
* Generating a working ``pyoxidizer.toml`` :ref:`configuration file <config_files>`.
* Telling the project's Rust build system about ``PyOxidizer``.

When that project's ``pyembed`` crate is built by Rust's build system, it calls
out to ``PyOxidizer`` to process the active ``PyOxidizer`` configuration file.
``PyOxidizer`` will obtain a specially-built Python distribution that is
optimized for embedding. It will then use this distribution to finish packaging
itself and any other Python dependencies indicated in the configuration file.
For example, you can process a pip requirements file at build time to include
additional Python packages in the produced binary.

At the end of this sausage grinder, ``PyOxidizer`` emits an archive library
containing Python (which can be linked into another library or executable)
and *resource files* containing Python data (such as Python module sources and
bytecode). Most importantly, ``PyOxidizer`` tells Rust's build system how to
integrate these components into the binary it is building.

From here, Rust's build system combines the standard Rust bits with the
files produced by ``PyOxidizer`` and turns everything into a binary,
typically an executable.

At run time, an instance of the ``PythonConfig`` struct from the ``pyembed``
crate is created to define how an embedded Python interpreter should behave.
(One of the build-time actions performed by ``PyOxidizer`` is to convert the
TOML configuration file into a default instance of this struct.) This struct
is used to instantiate a Python interpreter.

The ``pyembed`` crate implements a Python *extension module* which provides
custom module importing functionality. Light magic is used to coerce the
Python interpreter to load this module very early during initialization.
This allows the module to service Python ``import`` requests. The custom module
importer installed by ``pyembed`` supports retrieving data from a read-only
data structure embedded in the executable itself. Essentially, the Python
``import`` request calls into some Rust code provided by ``pyembed`` and
Rust returns a ``void *`` to memory containing data (module source code,
bytecode, etc) that was generated at build time by ``PyOxidizer`` and later
embedded into the binary by Rust's build system.

Once the embedded Python interpreter is initialized, the application works
just like any other Python application! The main differences are that modules
are (probably) getting imported from memory and that Rust - not the Python
distribution's ``python`` executable logic - is driving execution of Python.

.. _new_project_layout:

New Project Layout
==================

``pyoxidizer init`` essentially does two things:

1. Creates a new Rust executable project by running ``cargo init``.
2. Adds PyOxidizer files to that project.

If we run ``pyoxidizer init pyapp``, let's explore our newly-created ``pyapp``
project::

   $ find pyapp -type f | grep -v .git
   pyapp/Cargo.toml
   pyapp/src/main.rs
   pyapp/pyoxidizer.toml
   pyapp/pyembed/src/config.rs
   pyapp/pyembed/src/importer.rs
   pyapp/pyembed/src/data.rs
   pyapp/pyembed/src/lib.rs
   pyapp/pyembed/src/pyinterp.rs
   pyapp/pyembed/src/pyalloc.rs
   pyapp/pyembed/src/pystr.rs
   pyapp/pyembed/build.rs
   pyapp/pyembed/Cargo.toml

The Main Project
----------------

The ``Cargo.toml`` file is the configuration file for the Rust project.
Read more in
`the official Cargo documentation <https://doc.rust-lang.org/cargo/reference/manifest.html>`_.
The magic lines in this file to enable PyOxidizer are the following::

   [dependencies]
   pyembed = { path = "pyembed" }

These lines declare a dependency on the ``pyembed`` package in the directory
``pyembed``. ``Cargo.toml`` is overall pretty straightforward.

Next let's look at ``pyapp/src/main.rs``. If you aren't familiar with Rust
projects, the ``src/main.rs`` file is the default location for the source
file implementing an executable. If we open that file, we see a
``fn main() {`` line, which declares the *main* function for our executable.
The file is relatively straightforward. We import some symbols from the
``pyembed`` crate. We then construct a config object, use that to construct
a Python interpreter, then we run the interpreter and pass its exit code
to ``exit()``.

The ``pyembed`` Package
-----------------------

The bulk of the files in our new project are in the ``pyembed`` directory.
This directory defines a Rust project whose job it is to build and manage
an embedded Python interpreter. This project behaves like any other Rust
library project: there's a ``Cargo.toml``, a ``src/lib.rs`` defining the
main library define, and a pile of other ``.rs`` files implementing the
library functionality. The only functionality you will likely be concerned
about are the ``PythonConfig`` and ``MainPythonInterpreter` structs. These
types define how the embedded Python interpreter is configured and executed.
If you want to learn more about this crate and how it works, run ``cargo doc``.

There are a few special properties about the ``pyembed`` package worth
calling out.

First, the package is a copy of files from the PyOxidizer project. Typically,
one could reference a crate published on a package repository like
https://crates.io/ and we wouldn't need to have local files. However,
``pyembed`` is currently relying on modifications to some other published
crates (we plan to upstream all changes eventually). This means we can't
publish ``pyembed`` on crates.io. So we need to vendor a copy next to your
project. Sorry about the inconvenience!

Speaking of modification to the published crates, the ``pyembed``'s
``Cargo.toml`` enumerates those crates. If ``pyoxidizer`` was run from
an installed executable, these modified crates will be obtained from
PyOxidizer's canonical Git repository. If ``pyoxidizer`` was run out of
the PyOxidizer source repository, these modified crates will be obtained
from the local filesystem path to that repository. **You may want to
consider making copies of these crates and/or vendoring them next to your
project if you aren't comfortable fetching dependencies from the local
filesystem or a Git repository.**

Another property about ``pyembed`` worth mentioning is its ``build.rs`` build
script. This program runs as part of building the library. As you can
see from the source, this program attempts to locate a ``pyoxidizer``
executable and then calls ``pyoxidizer run-build-script``. ``pyoxidizer``
thus provides the bulk of the build script functionality. This is slightly
unorthodox. But it enables you to build applications without building all
of PyOxidizer. And since PyOxidizer has a few hundred package dependencies,
this saves quite a bit of time!

The ``pyoxidizer.toml`` Configuration File
------------------------------------------

The final file in our newly created project is ``pyoxidizer.toml``. **It is
the most important file in the project.**

The ``pyoxidizer.toml`` file configures how the embedded Python interpreter
is built. This includes choosing which modules to package. It also configures
the default run-time settings for the interpreter, including which code to
run.

See :ref:`config_files` for comprehensive documentation of ``pyoxidizer.toml``
files and their semantics.
