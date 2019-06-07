==========
PyOxidizer
==========

``PyOxidizer`` is a utility for producing binaries that embed Python.
The over-arching goal of ``PyOxidizer`` is to make complex packaging and
distribution problems simple so application maintainers can focus on
building applications instead of toiling with build systems and packaging
tools.

``PyOxidizer`` is capable of producing a single file executable - with
a copy of Python and all its dependencies statically linked and all
resources (like ``.pyc`` files) embedded in the executable. You can
copy a single executable file to another machine and run a Python
application contained within. It *just works*.

``PyOxidizer`` exposes its lower level functionality for embedding
self-contained Python interpreters as a tool and software library. So if
you don't want to ship executables that only consist of a Python
application, you can still use ``PyOxidizer`` to e.g. produce a library
containing Python suitable for linking in any application or use
``PyOxidizer``'s embedding library directly for embedding Python in a
larger application.

The *Oxidizer* part of the name comes from Rust: executables produced
by ``PyOxidizer`` are compiled from Rust and Rust code is responsible
for managing the embedded Python interpreter and all its operations.
If you don't know Rust, that's OK: PyOxidizer tries to make the existence
of Rust nearly invisible to end-users.

While solving packaging and distribution problems is the primary goal
of ``PyOxidizer``, a side-effect of solving that problem with Rust is
that ``PyOxidizer`` can serve as a bridge between these two languages.
``PyOxidizer`` can be used to easily add a Python interpreter to *any*
Rust project. But the opposite it also true: ``PyOxidizer`` can also be
used to add Rust to Python. Using ``PyOxidizer``, you could *bootstrap*
a new Rust project providing your application's executable/library.
Initially, that binary is a few lines of Rust that instantiates a Python
interpreter and runs Python code. Over time, functionality could be
(re)written in Rust and your previously Python-only project could
leverage Rust and its diverse ecosystem. Since ``PyOxidizer`` abstracts
the Python interpreter away, this could all be invisible to end-users:
you could rewrite an application from Python to Rust and people may
not even know because all they might see is a single file executable!

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
embedded Python interpreter. See the project documentation for info about
this file.
