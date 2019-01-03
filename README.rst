==========
PyOxidizer
==========

``PyOxidizer`` is a collection of Rust crates that facilitate building
libraries and binaries containing Python interpreters. ``PyOxidizer`` is
capable of producing a single file executable - with all dependencies
statically linked and all resources (like ``.pyc`` files) embedded in the
executable.

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
them from there).

Quick Start
===========

You need Rust 1.31 and a corresponding Cargo installed.

You will need a TOML configuration file telling us how to embed Python.
See ``docs/config.rst`` for documentation of this configuration file.

e.g.::

   [python_distribution]
   local_path = "/home/gps/src/python-build-standalone/build/cpython-linux64.tar.zst"
   sha256 = "5a43b90919672f8cc72a932b19f7627fa59ae66f867346f481ce01685db74fad"

   [python_config]
   program_name = "myprog"

   [python_packaging]
   module_paths = ["/home/gps/src/myapp/venv/lib/python3.7/site-packages"]

You can find available Python distributions at
https://github.com/indygreg/python-build-standalone/releases.

Then from a clone of this repository, run ``cargo build`` with the
``PYOXIDIZER_CONFIG`` environment variable pointing to this config file. e.g.

   $ PYOXIDIZER_CONFIG=~/src/myapp/pyoxidizer-linux.toml cargo build

This will build a ``target/debug/pyapp`` executable containing the configured
Python application. You can run it directly or with ``cargo run``.

Status of Project
=================

The project is considered technology preview status. It is not yet viable to
use in the wild. There are several missing features, bugs, and other rough
edges. Use at your own risk.

How It Works
============

``PyOxidizer`` ingests a specially produced Python distribution - likely
one from the [python-build-standalone](https://github.com/indygreg/python-build-standalone)
project). It consumes the ``libpython`` static library in its entirely
or the object files it was derived from. It also collects Python modules
from the standard library.

The Python standard library modules are supplemented with additional
``.py`` source files specified by the user. These are all assembled
into binary data structures and embedded in a Rust library, where the
raw module source and bytecode are exposed to Python via a minimal
extension module.

There exists a custom module importer that knows how to load modules
from the in-memory data structures exposed via the built-in extension
module. This module importer is implemented in pure Python.

This custom module importer is injected into the Python interpreter
by compiling custom bytecode for the ``importlib._bootstrap_external``
module and replacing its entry in the *frozen* modules array exposed
to Python's C API.

At run-time, Rust code instantiates an embedded Python interpreter with
our custom/defined settings.

The end result of this process can be as simple as a single, self-contained
executable. When the process is executed, very little work needs to be done
to run Python code, as all native code is available in the executable and
all ``.py`` and ``.pyc`` files can be loaded without performing any
explicit filesystem I/O.

Known Limitations and Planned Features
======================================

Only Python 3.7 is currently supported. Support for older Python 3
releases is possible. But the project author hopes we only need to
target the latest/greatest Python release.

There is no macOS or Windows support. Support is planned.

There is not yet support for controlling which Python C extensions
are linked into the final binary. Not all applications need every
Python C extension and removing C extensions could result in smaller
binaries. There are also licensing concerns with some extensions
(``gdbm`` and ``readline`` are GPL version 3).

There is not yet support for filtering which ``.py`` and ``.pyc``
files make it into the final binary. This is relatively trivial to
implement.

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

Repository Structure
====================

The ``pyrepackager`` directory contains a Rust crate with the build-time
code used for ingesting a Python distribution and emitting artifacts
and other configurations needed to produce an embeddable Python
interpreter. Because this is a build-time crate and doesn't contain
code for run-time, most of the logic for ``PyOxidizer`` lives in this
crate.

The ``pyembed`` directory defines a library Rust crate for interfacing
with an embedded Python interpreter. When built, this crate emits
resources for embedding a Python interpreter (custom module importer,
modules data structures, etc) and embeds them within the Rust library.

The ``pyapp`` directory defines a simple Rust crate which defines a
binary that uses the ``pyembed`` crate to instatiate and run an embedded
Python interpreter. This crate demonstrates how simple it is to integrate
and use a Python interpreter in an existing Rust project.

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

Frequently Asked Questions
==========================

Can Python 2.7 Be Supported?
----------------------------

In theory, yes. However, it is considerable more effort than Python 3. And
since Python 2.7 is being deprecated in 2020, in the project author's
opinion it isn't worth the effort.

Why Rust?
---------

``PyOxidizer`` requires a *driver* application to interface with the
Python C API and that *driver* application needs to compile to native
code. In the author's opinion, the only appropriate languages for this
were C, C++, and Rust.

Of those 3, the project's author prefers to write new projects in Rust
because it is a superior systems programming language that has learned
from decades of mistakes of its predecessors. The author prefers
technologies that can detect and eliminate entire classes of bugs
(like buffer overflow and use-after-free) at compile time.

Why is the Rust Code... Not Great?
----------------------------------

This is the project author's first real Rust project. Suggestions to improve
the Rust code would be very much appreciated!

Keep in mind that the ``pyrepackager`` crate is a build-time only
crate and arguably doesn't need to live up to quality standards as
crates containing run-time code. Things like aggressive ``.unwrap()``
usage are arguably tolerable.

What is the *Magic Sauce* That Makes PyOxidizer Special?
--------------------------------------------------------

There are 2 technical achievements that make ``PyOxidizer`` special.

First, ``PyOxidizer`` consumes Python distributions that were specially
built with the aim of being used for standalone/distributable applications.
These custom-built Python distributions are compiled in such a way that
the resulting binaries have very few external dependencies and run on
nearly every target system. Other tools that produce standalone Python
binaries often rely on an existing Python distribution, which often
doesn't have these characteristics.

Second is the ability to import ``.py``/``.pyc`` files from memory. Most
other self-contained Python applications rely on Python's ``zipimporter``
or do work at run-time to extract the standard library to a filesystem
(typically a temporary directory or a FUSE filesystem like SquashFS). What
``PyOxidizer`` does is expose the ``.py``/``.pyc`` modules data to the
Python interpreter via a Python extension module built-in to the binary.
In addition, the ``importlib._bootstrap_external`` module (which is
*frozen* into ``libpython``) is replaced by a modified version that
defines a custom module importer capable of loading Python modules
from the in-memory data structures exposed from the built-in extension
module.

The custom ``importlib_bootstrap_external`` frozen module trick is
probably the most novel technical achievement of ``PyOxidizer``. Other
Python distribution tools are encouraged to steal this idea!

Can Applications Import Python Modules from the Filesystem?
-----------------------------------------------------------

Yes. While the default is to import all Python modules from in-memory
data structures linked into the binary, it is possible to configure
``sys.path`` to allow importing from additional filesystem paths.
Support for importing compiled extension modules is also possible.

What are the Implications of Static Linking?
--------------------------------------------

Most Python distributions rely heavily on dynamic linking. In addition to
``python`` frequently loading a dynamic ``libpython``, many C extensions
are compiled as standalone shared libraries. This includes the modules
``_ctypes``, ``_json``, ``_sqlite3``, ``_ssl``, and ``_uuid``, which
provide the native code interfaces for the respective non-``_`` prefixed
modules which you may be familiar with.

These C extensions frequently link to other libraries, such as ``libffi1``,
``libsqlite3``, ``libssl``, and ``libcrypto``. And more often than not,
that linking is dynamic. And the libraries being linked to are provided
by the system/environment Python runs in. As a concrete example, on
Linux, the ``_ssl`` module can be provided by
``_ssl.cpython-36m-x86_64-linux-gnu.so``, which can have a shared library
dependency against ``libssl.so.1.1`` and ``libcrypto.so.1.1``, which
can be located in ``/usr/lib/x86_64-linux-gnu`` or a similar location
under ``/usr``.

When Python extensions are statically linked into a binary, the Python
extension code is part of the binary instead of in a standalone file.

If the extension code is linked against a static library, then the code
for that dependency library is part of the extension/binary instead of
dynamically loaded from a standalone file.

When ``PyOxidizer`` produces a fully statically linked binary, the code
for these 3rd party libraries is part of the produced binary and not
loaded from external files at load/import time.

There are a few important implications to this.

One is related to security and bug fixes. When 3rd party libraries are
provided by an external source (typically the operating system) and are
dynamically loaded, once the external library is updated, your binary
can use the latest version of the code. When that external library is
statically linked, you need to rebuild your binary to pick up the latest
version of that 3rd party library. So if e.g. there is an important
security update to OpenSSL, you would need to ship a new version of your
application with the new OpenSSL in order for users of your application
to be secure.

Another implication is code compatibility. If multiple consumers try
to use different versions of the same library... TODO

How is This Different From PyInstaller?
---------------------------------------

PyInstaller - like ``PyOxidizer`` - can produce a self-container executable
file containing your application. However, at run-time, PyInstaller will
extract Python source/bytecode files to a temporary directory then import
modules from the filesystem. ``PyOxidizer`` skips this step and loads
modules directly from memory.

How is This Different From py2exe?
----------------------------------

TODO

How is This Different From Shiv?
--------------------------------

`Shiv <https://shiv.readthedocs.io/en/latest/>`_ is a packager for zip file
based Python applications. The Python interpreter has built-in support for
running self-contained Python applications that are distributed as zip files.

Shiv requires the target system to have a Python executable and for the target
to support shebangs in executable files. This is acceptable for controlled
*NIX environments. It isn't acceptable for Windows (which doesn't support
shebangs) nor for environments where you can't guarantee an appropriate
Python executable is available.

Also, by distributing our own Python interpreter with the application, we
have stronger guarantees about the run-time environment. For example, you
can aggressively target the latest Python version. Another benefit of
distributing our own Python interpreter is we can run a Python interpreter
with various optimizations, such as profile-guided optimization (PGO) and
link-time optimization (LTO). We can also easily configure custom memory
allocators or tweak memory allocators for optimal performance.

How is This Different From PEX?
-------------------------------

`PEX <https://github.com/pantsbuild/pex>`_ is a packager for zip file based
Python applications. For purposes of comparison, PEX and Shiv have the
same properties.

How is This Different From XAR?
-------------------------------

`XAR <https://github.com/facebookincubator/xar/>`_ requires the use of SquashFS.
SquashFS requires Linux.

``PyOxidizer`` is a target native executable and doesn't require any special
filesystems or other properties to run.

How is This Different From Docker / Running a Container
-------------------------------------------------------

It is increasingly popular to distribute applications as self-contained
container environments. e.g. Docker images. This distribution mechanism
is effective for Linux users.

``PyOxidizer`` will likely produce a smaller distribution than container-based
applications. This is because many container-based applications contain a lot
of extra content that isn't needed by the processes within.

``PyOxidizer`` also doesn't require a container execution environment. Not
every user has the capability to run certain container formats. However,
nearly every user can run a self-contained executable.

How is This Different From Nuitka?
----------------------------------

`Nuitka <http://nuitka.net/pages/overview.html>`_ can compile Python programs
to single executables. And the emphasis is on *compile*: Nuitka actually
converts Python to C and compiles that. Nuitka is effectively an alternate
Python interpreter.

Nuitka is a cool project and purports to produce significant speed-ups
compared to CPython.

Since Nuitka is effectively a new Python interpreter, there are risks to
running Python in this environment. Some code has dependencies on CPython
behaviors. There may be subtle bugs are lacking features from Nuitka.
However, Nuitka supposedly supports every Python construct, so many
applications should *just work*.

Given the performance benefits of Nuitka, it is a compelling alternative
to ``PyOxidizer``.

How is This Different From PyRun?
---------------------------------

`PyRun <https://www.egenix.com/products/python/PyRun>`_ can produce single
file executables. The author isn't sure how it works. PyRun doesn't
appear to support modern Python versions. And it appears to require shared
libraries (like bzip2) on the target system. ``PyOxidizer`` supports
the latest Python and doesn't require shared libraries that aren't in
nearly every environment.
