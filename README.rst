==========
PyOxidizer
==========

``PyOxidizer`` is a utility for producing binaries that embed Python.
``PyOxidizer`` is capable of producing a single file executable - with
all dependencies statically linked and all resources (like ``.pyc``
files) embedded in the executable.

The over-arching goal of ``PyOxidizer`` is to make complex packaging and
distribution problems simple so application maintainers can focus on
building applications instead of toiling with build systems and packaging
tools.

The *Oxidizer* part of the name comes from Rust: executables produced
by ``PyOxidizer`` are compiled from Rust and Rust code is responsible
for managing the embedded Python interpreter and all its operations.

``PyOxidizer`` is similar in nature to
`PyInstaller <http://www.pyinstaller.org/>`_,
`Shiv <https://shiv.readthedocs.io/en/latest/>`_, and other tools in
this space. What generally sets ``PyOxidizer`` apart is that produced
executables contain an embedded, statically-linked Python interpreter,
have no additional run-time dependency on the target system (e.g.
minimal dependencies on shared libraries, container runtimes, or
FUSE filesystems), and runs everything from memory (as opposed to
e.g. extracting Python modules to a temporary directory and loading
them from there). This makes binaries produced with ``PyOxidizer``
faster and simpler to manage.

Quick Start
===========

You need Rust 1.31+ and a corresponding Cargo installed. Then::

   # PyOxidizer must be installed from a Git repository. This is
   # temporary until things are stable enough for a release on
   # ``crates.io``.
   $ git clone https://github.com/indygreg/PyOxidizer
   $ cd PyOxidizer

   # Build and install the ``pyoxidizer`` executable. This will take
   # a while because there are a number of dependencies. These dependencies
   # are for running ``pyoxidizer`` and don't impact the size of binaries
   # built with PyOxidizer.
   $ cargo install --path pyoxidizer

   # Verify the `pyoxidizer` executable is installed.
   $ pyoxidizer help

   # Create a new Rust project using PyOxidizer.
   #
   # This will call ``cargo init`` and set up PyOxidizer scaffolding in the
   # new project.
   $ pyoxidizer init /path/to/my-project

   # Build our application.
   $ cd /path/to/my-project
   $ cargo build

   # When building, you may want to inspect the ``pyoxidizer.toml`` file
   # in your project's directory to see what can be customized.

   # And run it. You should get a Python REPL as if you had invoked
   # `python` on the command line.
   $ cargo run

   # (Optional) Build a non-debug, release-optimized binary.
   $ cargo build --release

   # Analyze the binary dependencies of the binary so you can evaluate
   # whether it is safe to distribute.
   $ pyoxidizer analyze target/debug/my-app

PyOxidizer uses TOML configuration files describing how to configure the
embedded Python interpreter. See the ``pyoxidizer`` crate documentation
for info about this file.

The TOML configuration file is processed as part of building the
``pyembed`` crate, which is the crate that manages an embedded Python
interpreter. The build script for the ``pyembed`` crate will use the
configuration file defined by the ``PYOXIDIZER_CONFIG`` environment
variable and fall back to looking for a ``pyoxidizer.toml`` file
in the directory ancestry of the ``pyembed`` crate.

Status of Project
=================

PyOxidizer is in alpha status. It may work for some use cases. However, there
are still a number of rough edges, missing features, and known limitations.
Please file GitHub issues!

What Works:

* ``pyoxidizer init`` and the project it creates should work on Linux,
  Windows, and macOS.
* TOML configuration allows cherry pick exactly which Python modules
  and extension to use.

What Doesn't Work:

* The ``importlib.abc.ResourceReader`` interface is not yet supported.
* Bundling compiled extension modules is not yet supported (e.g. C
  extensions).
* Error handling in Rust code isn't great. Expect binaries to crash
  from time to time or with esoteric input.
* ``pyoxidizer add`` doesn't work well.
* There is no ``pyoxidizer update`` yet.
* ``pyoxidizer analyze`` only works on ELF files (read: no Windows or
  macOS support).

The biggest risks to distributing binaries produced with PyOxidizer is
likely general instability (mainly due to not great error handling yet)
and binary compatibility concerns. To ensure maximum binary compatibility
on Linux, compile your binary in a Debian 7 environment, as this will use
a sufficiently old version of libc which should work in most Linux
environments. Of course, if you control the execution environment (like if
executables will run on the same machine that built them), then this may
not pose a problem to you. Use the ``pyoxidizer analyze`` command to
inspect binaries for compatibility.

How It Works
============

``PyOxidizer`` is comprised of a number of Rust crates, each responsible
for particular functionality.

The ``pyoxidizer`` crate provides a ``pyoxidizer`` executable and library.
The library provides all the core functionality of PyOxidizer, such as
the logic for ingesting specially produced Python distributions and
enabling those distributions to be repackaged and embedded in a Rust
binary. It has code for parsing our config files, finding Python modules,
compiling Python bytecode, etc. The ``pyoxidizer`` executable serves
as a high-level interface to performing actions relevant to PyOxidizer.

The ``pyembed`` library crate is responsible for managing an embedded
Python interpreter within a larger Rust application. The crate contains
all the code needed to interact with the CPython APIs and to provide
in-memory module importing.

When built, the ``pyembed`` crate interacts with the ``pyoxidizer`` crate
to assemble all resources required to embed a Python interpreter. This
includes configuring Cargo to build/link the appropriate files to embed
``libpython``. This activity is directed by a configuration file. See the
crate's ``build.rs`` for more.

A built ``pyembed`` crate contains a default configuration (derived from
the ``build.rs`` program) for the embedded Python interpreter. However,
this configuration does not need to be used and the API exposed by the
``pyembed`` crate allows custom behavior not matching these defaults.

The ``pyembed`` create is configured via a TOML file. The configuration
defines which Python distribution to consume, which Python modules to
package, and default settings for the Python interpreter, including which
code to execute by default. Most of the reading and processing of this
configuration is in the ``pyoxidizer`` crate.

At build time, the ``pyembed`` crate assembles configured Python
resources (such as ``.py`` source files and bytecode) into binary structures
and exposes this data to the ``pyembed`` crate via ``const &'static [u8]``
variables. At run time, these binary arrays are parsed into richer Rust data
structures, which allow Rust to access e.g. the Python bytecode for
a named Python module. The embedded Python interpreter contains a
custom *built-in extension module* which implements a Python meta path
importer that services import requests. In order to make this importer
available to the Python interpreter, at ``pyembed`` build time, we
compile a modified version of the ``importlib._bootstrap_external`` module
(provided by the Python distribution) to Python bytecode. When the embedded
Python interpreter is initialized, this custom bytecode calls into
our *built-in extension module*, which installs itself, and allows the
entirety of the Python standard library and custom modules to be imported from
memory using zero-copy.

The final output of PyOxidizer can be as simple as a single, self-contained
executable containing Python and all its required modules. When the
process is executed, very little work needs to be done to run Python code,
as Python modules can be imported from memory without explicit filesystem
I/O.

Known Limitations and Planned Features
======================================

Segfaults on shutdown are a known problem. This should hopefully be
resolved soon.

Only Python 3.7 is currently supported. Support for older Python 3
releases is possible. But the project author hopes we only need to
target the latest/greatest Python release.

The TOML config files and how crates are built needs some work.

There is not yet support for reordering ``.py`` and ``.pyc`` files
in the binary. This feature would facilitate linear read access,
which could lead to faster execution.

Binary resources are currently stored as raw data. They could be
stored compressed to keep binary size in check (at the cost of run-time
memory usage and CPU overhead).

There is not yet support for lazy module importers. Even though importing
is faster due to no I/O, a large part of module importing is executing
module code on import. So lazy module importing is still beneficial.
``PyOxidizer`` will eventually ship a built-in lazy module importer.
There are also possibilities for alternate module serialization techniques
which are faster than ``marshal``. Some have experimented with serializing
the various ``PyObject`` types and adjusting pointers at run-time...

The `ResourceReader <https://docs.python.org/3.7/library/importlib.html#importlib.abc.ResourceReader>`_
API for loading resources is not yet implemented. This appears to be the
recommended way to access non-module data from packages. We will definitely
support this API someday.

There is not yet support for integrating custom extension modules (compiled
Python extensions). This should be doable, assuming those extensions are
compiled with the same toolchain used to produce the embedded Python
interpreter. We make that toolchain available for download and can likely
automate the building of custom extension modules.

Windows currently requires a Nightly Rust to build (you can set the
environment variable ``RUSTC_BOOTSTRAP=1`` to work around this) because
the ``static-nobundle`` library type is required.
https://github.com/rust-lang/rust/issues/37403 tracks making this feature
stable. It *might* be possible to work around this by adding an
``__imp_`` prefixed symbol in the right place or by producing a empty
import library to satisfy requirements of the ``static`` linkage kind.
See
https://github.com/rust-lang/rust/issues/26591#issuecomment-123513631 for
more.

Cross compiling is not yet supported.

Licensing Considerations
========================

Python and its various dependencies are governed by a handful of licenses.
These licenses have various requirements and restrictions.

Currently, binaries produced with ``PyOxidizer`` contain statically linked
code covered by various licenses. This includes GPL 3.0 licensed code
(``libreadline`` and ``libgdbm``). This has significant implications!

In the future, ``PyOxidizer`` will allow stripping components of the Python
distribution that have undesirable licenses and may allow distributing
specific components as standalone libraries to skirt around some licensing
restrictions.
