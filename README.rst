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
them from there). This makes binaries produced with ``PyOxidizer``
faster and simpler to manage.

Quick Start
===========

You need Rust 1.31+ and a corresponding Cargo installed.

You will need a TOML configuration file telling us how to embed Python.
See ``docs/config.rst`` for documentation of this configuration file.

e.g.::

   [python_distribution]
   url = "https://github.com/indygreg/python-build-standalone/releases/download/20190427/cpython-3.7.3-linux64-20190427T2308.tar.zst"
   sha256 = "0b30af0deb4852f2099c7905f80f55b70f7eec152cd19a3a65b577d4350ad47a"

   [python_config]
   program_name = "myprog"

   [python_extensions]
   policy = "all"

   [[python_packages]]
   type = "stdlib"

   [[python_packages]]
   type = "virtualenv"
   path = "/home/gps/venv-myprog"

   [python_run]
   mode = "repl"

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

``PyOxidizer`` is comprised of a number of Rust crates, each responsible
for particular functionality.

The ``pyrepackager`` crate contains functionality for ingesting specially
produced Python distributions - likely one from the
[python-build-standalone](https://github.com/indygreg/python-build-standalone)
project) and enabling those distributions to be repackaged. It has code
for parsing our config files, finding Python modules, compiling Python
bytecode, etc. ``pyrepackager`` is essentially the main library for
``PyOxidizer``, providing most of the build-time functionality required
to build binaries.

The ``pyembed`` library crate is responsible for managing an embedded
Python interpreter within a larger Rust application. The crate contains
all the code needed to interact with the CPython APIs and to provide
in-memory module importing.

When built, the ``pyembed`` crate interacts with the ``pyrepackager`` crate
to assemble all resources required to embed a Python interpreter. This
includes configuring Cargo to build/link the appropriate files to embed
``libpython``. This activity is directed by a configuration file. See the
crate's ``build.rs`` for more.

A built ``pyembed`` crate contains a default configuration (derived from
the ``build.rs`` program) for the embedded Python interpreter. However,
this configuration does not need to be used and the API exposed by the
``pyembed`` crate allows custom behavior not matching these defaults.

A ``pyapp`` crate defines a Rust program that simply calls into the
``pyembed`` crate and instantiates and runs Python with the configured
default settings. The crate exists for convenience to facilitate testing
and to demonstrate how Rust applications can interact with the ``pyembed``
crate.

The ``pyembed`` create is configured via a TOML file. The configuration
defines which Python distribution to consume, which Python modules to
package, and default settings for the Python interpreter, including which
code to execute by default. Most of the reading and processing of this
configuration is in the ``pyrepackager`` crate.

At build time, the ``pyembed`` crate assembles configured Python
resources (such as ``.py`` source files and bytecode) into binary structures
and exposes this data to the ``pyembed`` crate via ``const &'static [u8]``
variables. At run time, these binary arrays are parsed into richer Rust data
structures, which allow Rust to access e.g. the Python bytecode for
a named Python module. The embedded Python interpreter contains a
custom *built-in extension module* which exposes these Rust data
structures to Python as the ``_pymodules`` module. There exists a pure
Python meta path importer providing an
``importlib.abc.MetaPathFinder``/``importlib.abc.Loader`` which uses the
``_pymodules`` extension module to provide access to Python source,
code, and resource data. In order to make this importer available to
the Python interpreter, at ``pyembed`` build time, the Python source
code for this importer is concatenated with the
``importlib._bootstrap_external`` module (provided by the Python
distribution) and compiled into Python bytecode. When the embedded
Python interpreter is initialized, this custom bytecode is used
to *bootstrap* the Python importing mechanism, allowing the entirety
of the Python standard library and custom modules to be imported from
memory using zero-copy access to the Python bytecode.

The final output of PyOxidizer can be as simple as a single, self-contained
executable containing Python and all its required modules. When the
process is executed, very little work needs to be done to run Python code,
as Python modules can be imported from memory without explicit filesystem
I/O.

Known Limitations and Planned Features
======================================

Only Python 3.7 is currently supported. Support for older Python 3
releases is possible. But the project author hopes we only need to
target the latest/greatest Python release.

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
because it is a superior systems programming language that has built on
lessons learned from decades working with its predecessors.The author
prefers technologies that can detect and eliminate entire classes of bugs
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
