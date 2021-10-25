.. py:currentmodule:: starlark_pyoxidizer

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

.. _faq_python_38:

Why is Python 3.8 Required?
===========================

Python 3.8 contains a new C API for controlling how embedded Python
interpreters are started. This makes the run-time code that native
binaries execute much, much simpler.

PyOxidizer versions up to 0.7 supported Python 3.7. But a decision
was made to require Python 3.8 because the run-time code to manage
the Python interpreter was vastly simpler and less prone to bugs.
Given that Python 3.8 is mostly backwards compatible with Python 3.7,
this wasn't perceived as a significant annoyance.

``No python interpreter found of version 3.*`` Error When Building
==================================================================

This is due to a dependent crate insisting that a Python executable
exist on ``PATH``. Set the ``PYO3_PYTHON`` environment variable to
the path of a Python 3.8+ executable and try again. e.g.::

   # UNIX
   $ export PYO3_PYTHON=/usr/bin/python3.9
   # Windows
   $ SET PYO3_PYTHON=c:\python39\python.exe

.. note::

   The ``pyoxidizer`` tool should take care of setting ``PYO3_PYTHON``
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
:py:class:`PythonInterpreterConfig`).

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

``vcruntime140.dll was not found`` Error on Windows
===================================================

Binaries built with PyOxidizer often have a dependency on the Visual
C++ Redistributable Runtime, or ``vcruntime140.dll``. If this file
is not present on your system or in a path where the built binary
can find it, you'll get an error about this missing file when attempting to
run/load the binary.

PyOxidizer has some support for managing this file for you. See
:ref:`pyoxidizer_distributing_windows_vc_redist` for more.

If PyOxidizer is not materializing this file next your built binary,
either you've disabled this functionality via your configuration
file (see :py:attr:`PythonExecutable.windows_runtime_dlls_mode`)
or PyOxidizer could not find the Visual Studio component providing this
file.

The quick fix for this is to install the Visual C++ Redistributable
runtime globally on your system. Simply go to
https://support.microsoft.com/en-us/topic/the-latest-supported-visual-c-downloads-2647da03-1eea-4433-9aff-95f26a218cc0
and download and install the appropriate platform installer for
``Visual Studio 2015, 2017 and 2019``.

If you want PyOxidizer to materialize the DLL(s) next to your built
binary, you'll need to install Visual Studio with the
``Microsoft.VisualCPP.Redist.14.Latest`` component (you will typically
get this component if installing support for building C/C++ applications).

``ld: unsupported tapi file type '!tapi-tbd' in YAML file`` on macOS When Building
==================================================================================

If you see this error when building on macOS, it means that the linker (likely
Clang) being used is not able to read the ``.tbd`` files from a more modern
Apple SDK.

PyOxidizer requires using an Apple SDK no older than the one used to build
the Python distributions being embedded (see
:ref:`pyoxidizer_distributing_macos_build_machine_requirements`). So the only
recourse to this problem is to use a more modern linker.

On Apple platforms, it is common to use the clang/linker from an Xcode or
Xcode Commandline Tools install. So the problem can usually be fixed by
upgrading Xcode or the Xcode Commandline Tools.
