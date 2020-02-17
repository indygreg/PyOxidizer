.. _faq:

==========================
Frequently Asked Questions
==========================

Where Can I Report Bugs / Send Feedback / Request Features?
===========================================================

At https://github.com/indygreg/PyOxidizer/issues

.. _faq_why_another_tool:

Why Build Another Python Application Packaging Tool?
====================================================

It is true that several other tools exist to turn Python code into distributable applications!
:ref:`comparisons` attempts to exhaustively compare ``PyOxidizer``
to these myriad of tools. (If a tool is missing or the comparison incomplete
or unfair, please file an issue so Python application maintainers can make
better, informed decisions!)

The long version of how ``PyOxidizer`` came to be can be found in the
`Distributing Standalone Python Applications <https://gregoryszorc.com/blog/2018/12/18/distributing-standalone-python-applications/>`_
blog post. If you really want to understand the motivations for
starting a new project rather than using or improving an existing
one, read that post.

If you just want the extra concise version, at the time ``PyOxidizer``
was conceived, there were no Python application packaging/distribution
tool which satisfied **all** of the following requirements:

* Works across all platforms (many tools target e.g. Windows or macOS only).
* Does not require an already-installed Python on the executing system
  (rules out e.g. zip file based distribution mechanisms).
* Has no special system requirements (e.g. SquashFS, container runtimes).
* Offers startup performance no worse than traditional ``python`` execution.
* Supports single file executables with none or minimal system dependencies.

Can Python 2.7 Be Supported?
============================

In theory, yes. However, it is considerable more effort than Python 3. And
since Python 2.7 is being deprecated in 2020, in the project author's
opinion it isn't worth the effort.

``No python interpreter found of version 3.*`` Error When Building
==================================================================

This is due to a dependent crate insisting that a Python executable
exist on ``PATH``. Set the ``PYTHON_SYS_EXECUTABLE`` environment
variable to the path of a Python 3.7 executable and try again. e.g.::

   # UNIX
   $ export PYTHON_SYS_EXECUTABLE=/usr/bin/python3.7
   # Windows
   $ SET PYTHON_SYS_EXECUTABLE=c:\python37\python.exe

.. note::

   The ``pyoxidizer`` tool should take care of setting ``PYTHON_SYS_EXECUTABLE``
   and prevent this error. If you see this error and you are building with
   ``pyoxidizer``, it is a bug that should be reported.

Why Rust?
=========

This is really 2 separate questions:

* Why choose Rust for the run-time/embedding components?
* Why choose Rust for the build-time components?

``PyOxidizer`` binaries require a *driver* application to interface with
the Python C API and that *driver* application needs to compile to native
code in order to provide a *native* executable without requiring a run-time
on the machine it executes on. In the author's opinion, the only appropriate
languages for this were C, Rust, and maybe C++.

Of those 3, the project's author prefers to write new projects in Rust
because it is a superior systems programming language that has built on
lessons learned from decades working with its predecessors. The author
prefers technologies that can detect and eliminate entire classes of bugs
(like buffer overflow and use-after-free) at compile time. On a less-opinionated
front, Rust's built-in build system support means that we don't have to
spend considerable effort solving hard problems like cross-compiling.
Implementing the embedding component in Rust also creates interesting
opportunities to embed Python in Rust programs. This is largely an
unexplored area in the Python ecosystem and the author hopes that PyOxidizer
plays a part in more people embedding Python in Rust.

For the non-runtime packaging side of ``PyOxidizer``, pretty much any
programming language would be appropriate. The project's author initially
did prototyping in Python 3 but switched to Rust for synergy with the the
run-time driver and because Rust had working solutions for several systems-level
problems, such as parsing ELF, DWARF, etc executables, cross-compiling,
integrating custom memory allocators, etc. A minor factor was the author's
desire to learn more about Rust by starting a *real* Rust project.

Why is the Rust Code... Not Great?
==================================

This is the project author's first real Rust project. Suggestions to improve
the Rust code would be very much appreciated!

Keep in mind that the ``pyoxidizer`` crate is a build-time only
crate and arguably doesn't need to live up to quality standards as
crates containing run-time code. Things like aggressive ``.unwrap()``
usage are arguably tolerable.

The run-time code that produced binaries run (``pyembed``) is held to
a higher standard and is largely ``panic!`` free.

What is the *Magic Sauce* That Makes PyOxidizer Special?
========================================================

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

Following the *Documentation* link for the
`pyembed <https://crates.io/crates/pyembed>`_ crate for an overview of how
the in-memory import machinery works.

Can Applications Import Python Modules from the Filesystem?
===========================================================

Yes. While the default is to import all Python modules from in-memory
data structures linked into the binary, it is possible to configure
``sys.path`` to allow importing from additional filesystem paths.
Support for importing compiled extension modules is also possible.

What are the Implications of Static Linking?
============================================

Most Python distributions rely heavily on dynamic linking. In addition to
``python`` frequently loading a dynamic ``libpython``, many C extensions
are compiled as standalone shared libraries. This includes the modules
``_ctypes``, ``_json``, ``_sqlite3``, ``_ssl``, and ``_uuid``, which
provide the native code interfaces for the respective non-``_`` prefixed
modules which you may be familiar with.

These C extensions frequently link to other libraries, such as ``libffi``,
``libsqlite3``, ``libssl``, and ``libcrypto``. And more often than not,
that linking is dynamic. And the libraries being linked to are provided
by the system/environment Python runs in. As a concrete example, on
Linux, the ``_ssl`` module can be provided by
``_ssl.cpython-37m-x86_64-linux-gnu.so``, which can have a shared library
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
to be secure. This shifts the security onus from e.g. your operating
system vendor to you. This is less than ideal because security updates
are one of those problems that tend to benefit from greater centralization,
not less.

It's worth noting that PyOxidizer's library security story is the
same as it is for e.g. Docker images. Docker images have the same
security properties. If you are OK distributing Docker images, you
should be OK with distributing executables built with PyOxidizer.

Another implication of static linking is licensing considerations. Static
linking can trigger stronger licensing protections and requirements.
Read more at :ref:`licensing_considerations`.

``error while loading shared libraries: libcrypt.so.1: cannot open shared object file: No such file or directory`` When Building
================================================================================================================================

If you see this error when building, it is because your Linux system does not
conform to the
`Linux Standard Base Specification <https://refspecs.linuxfoundation.org/LSB_5.0.0/LSB-Core-AMD64/LSB-Core-AMD64/libcrypt.html>`_,
does not provide a ``libcrypt.so.1`` file, and the Python distribution that
PyOxidizer attempts to run to compile Python source modules to bytecode can't
execute.

Fedora 30+ are known to have this issue. A workaround is to install the
``libxcrypt-compat`` on the machine running ``pyoxidizer``. See
https://github.com/indygreg/PyOxidizer/issues/89 for more info.
