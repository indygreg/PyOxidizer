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

During Python interpreter initialization, a custom Rust-implemented
Python importer is registered and takes over all imports. Requests for
modules are serviced from the parsed data structure defining known
modules.

Follow the *Documentation* link for the
`pyembed <https://crates.io/crates/pyembed>`_ crate for an overview of how
the in-memory import machinery works.

Can Applications Import Python Modules from the Filesystem?
===========================================================

Yes!

While PyOxidizer supports importing Python resources from
in-memory, it also supports filesystem-based import like
traditional Python applications.

This can be achieved by adding Python resources to a non
*in-memory* resource location (see :ref:`packaging_resources`) or
by enabling Python's standard filesystem-based importer by
enabling ``filesystem_importer=True`` (see
:ref:`config_python_interpreter_config`).

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
