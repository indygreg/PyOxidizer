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
