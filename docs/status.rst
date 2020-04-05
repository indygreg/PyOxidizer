.. _project_status:

==============
Project Status
==============

PyOxidizer is functional and works for many use cases. However, there
are still a number of rough edges, missing features, and known limitations.
Please file issues at https://github.com/indygreg/PyOxidizer/issues!

What's Working
==============

The basic functionality of creating binaries that embed a self-contained
Python works on Linux, Windows, and macOS. The general approach should
work for other operating systems.

Starlark configuration files allow extensive customization of packaging and
run time behavior. Many projects can be successfully packaged with
PyOxidizer today.

Major Missing Features
======================

An Official Build Environment
-----------------------------

Compiling binaries that work on nearly every target system is hard.
On Linux, things like ``glibc`` symbol versions from the build machine
can leak into the built binary, effectively requiring a new Linux
distribution to run a binary.

In order to make the binary build process robust, we will need to
provide an execution environment in which to build portable binaries.
On Linux, this likely
entails making something like a Docker image available. On Windows and
macOS, we might have to provide a tarball. In all cases, we want this
environment to be integrated into ``pyoxidizer build`` so end users
don't have to worry about jumping through hoops to build portable
binaries.

.. _status_extension_modules:

Native Extension Modules
------------------------

Using compiled extension modules (e.g. C extensions) is partially
supported.

Building C extensions to be embedded in the produced binary works
for Windows, Linux, and macOS.

Support for extension modules that link additional macOS frameworks
not used by Python itself is not yet implemented (but should be easy to
do).

Support for cross-compiling extension modules (including to MUSL) does
not work. (It may appear to work and break at linking or run-time.)

We also do not yet provide a build environment for C extensions. So
unexpected behavior could occur if e.g. a different compiler toolchain
is used to build the C extensions from the one that produced the
Python distribution.

See also :ref:`pitfall_extension_modules`.

Incomplete ``pyoxidizer`` Commands
----------------------------------

``pyoxidizer add`` and ``pyoxidizer analyze`` aren't fully implemented.

There is no ``pyoxidizer upgrade`` command.

Work on all of these is planned.

More Robust Packaging Support
-----------------------------

Currently, we produce an executable via Cargo. Often a self-contained
executable is not suitable. We may have to run some Python modules from
the filesystem because of limitations in those modules. In addition, some
may wish to install custom files alongside the executable.

We want to add a myriad of features around packaging functionality to
facilitate these things. This includes:

* Support for ``__file__``.
* A build mode that produces an instrumented binary, runs it a few times
  to dump loaded modules into files, then builds it again with a pruned
  set of resources.

Making Distribution Easy
------------------------

We don't yet have a good story for the *distributing* part of the application
distribution problem. We're good at producing executables. But we'd like to
go the extra mile and make it easier for people to produce installers, ``.dmg``
files, tarballs, etc.

This includes providing build environments for e.g. non-MUSL based Linux
executables.

It also includes support for auditing for license compatibility (e.g. screening
for GPL components in proprietary applications) and assembling required license
texts to satisfy notification requirements in those licenses.

Partial Terminfo and Readline Support
-------------------------------------

PyOxidizer has partial support for detecting ``terminfo`` databases. See
:ref:`terminfo_database` for more.

There's a good chance PyOxidizer's ability to locate ``terminfo`` databases
in the long tail of Python distributions is lacking. And PyOxidizer doesn't
currently make it easy to distribute a ``terminfo`` database alongside the
application.

At this time, proper terminal interaction in PyOxidizer applications may be
hit-or-miss.

Please file issues at https://github.com/indygreg/PyOxidizer/issues reporting
known problems with terminal interaction or to request new features for
terminal interaction, ``terminfo`` database support, etc.

Lesser Missing Features
=======================

Python Version Support
----------------------

Only Python 3.7 is currently supported. Support for older Python 3
releases is possible. But the project author hopes we only need to
target the latest/greatest Python release.

Reordering Resource Files
-------------------------

There is not yet support for reordering ``.py`` and ``.pyc`` files
in the binary. This feature would facilitate linear read access,
which could lead to faster execution.

Compressed Resource Files
-------------------------

Binary resources are currently stored as raw data. They could be
stored compressed to keep binary size in check (at the cost of run-time
memory usage and CPU overhead).

Nightly Rust Required on Windows
--------------------------------

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

Cross Compiling
---------------

Cross compiling is not yet supported. We hope to and believe we can
support this someday. We would like to eventually get to a state where you
can e.g. produce Windows and macOS executables from Linux. It's possible.

Configuration Files
-------------------

Naming and semantics in the configuration files can be significantly
improved. There's also various missing packaging functionality.

Eventual Features
=================

The immediate goal of ``PyOxidizer`` is to solve packaging and distribution
problems for Python applications. But we want ``PyOxidizer`` to be more than
just a packaging tool: we want to add additional features to ``PyOxidizer``
to bring extra value to the tool and to demonstrate and/or experiment with
alternate ways of solving various problems that Python applications
frequently encounter.

Lazy Module Loading
-------------------

When a Python module is imported, its code is evaluated. When applications
consist of dozens or even hundreds of modules, the overhead of executing all
this code at ``import`` time can be substantial and add up to dozens of
milliseconds of overhead - all before your application runs a meaningful line
of code.

We would like ``PyOxidizer`` to provide lazy module importing so Python's
``import`` machinery can defer evaluating a module's code until it is actually
needed. With features in modern versions of Python 3, this feature could likely
be enabled by default. And since many ``PyOxidizer`` applications are
*frozen* and have total knowledge of all importable modules at build time,
``PyOxidizer`` could return a *lazy* module object after performing a simple
Rust ``HashMap`` lookup. This would be extremely fast.

Alternate Module Serialization Techniques
-----------------------------------------

Related to lazy module loading, there is also the potential to explore
alternate module serialization techniques. Currently, the way ``PyOxidizer``
and ``.pyc`` files work is that a Python code object is serialized with the
``marshal`` module. At module load time, the code object is deserialized
and then executed. This deserialization plus code execution has overhead.

It is possible to devise alternate serialization and load techniques that
don't rely on ``marshal`` and possibly bypass having to run as much code
at module load time. For example, one could devise a format for serializing
various ``PyObject`` types and then adjusting pointers inside the structs
at run time. This is kind of a crazy idea. But it could work.

Module Order Tracing
--------------------

Currently, resource data is serialized on disk in alphabetical order according
to the resource name. e.g. the ``bar`` module is serialized before the ``foo``
module.

We would like to explore a mechanism to record the order in which modules are
loaded as part of application execution and then reorder the serialized modules
such that they are stored in load order. This will facilitate linear reads at
application run time and possibly provide some performance wins (especially on
devices with slow I/O).

Module Import Performance Tracing
---------------------------------

``PyOxidizer`` has near total visibility into what Python's module importer
is doing. It could be very useful to provide forensic output of what modules
import what, how long it takes to import various modules, etc.

CPython does have some support for module importing tracing. We think we can
go a few steps farther. And we can implement it more easily in Rust than
what CPython can do in C. For example, with Rust, one can use the
`inferno crate <https://github.com/jonhoo/inferno>`_ to emit flame graphs
directly from Rust, without having to use external tools.

Built-in Profiler
-----------------

There's potential to integrate a built-in profiler into ``PyOxidizer``
applications. The excellent `py-spy <https://github.com/benfred/py-spy>`_
sampling profiler (or the core components of it) could potentially be
integrated directly into ``PyOxidizer`` such that produced applications
could self-profile with minimal overhead.

It should also be possible for ``PyOxidizer`` to expose mechanisms for
Rust to receive callbacks when Python's
`profiling and tracing <https://docs.python.org/3.7/c-api/init.html#profiling-and-tracing>`_
hooks fire. This could allow building a powerful debugger or tracer
in Rust.

Command Server
--------------

A known problem with Python is its startup overhead. The maintainer of
``PyOxidizer`` has raised this issue on Python's mailing list
`a <https://mail.python.org/pipermail/python-dev/2014-May/134528.html>`_
`few <https://mail.python.org/pipermail/python-dev/2018-May/153296.html>`_
`times <https://mail.python.org/pipermail/python-dev/2018-October/155466.html>`_.

``PyOxidizer`` helps with this problem by eliminating explicit filesystem I/O
and allowing modules to be imported faster. But there's only so much that can
be done and startup overhead can still be a problem.

One strategy to combat this problem is the use of persistent *command
server daemons*. Essentially, on the first invocation of a program you
spawn a background process running Python. That process listens for
*command requests* on a pipe, socket, etc. You send the current command's
arguments, environment variables, other state, etc to the background process.
It uses its Python interpreter to execute the command and send results back
to the main process. On the 2nd invocation of your program, the Python
process/interpreter is already running and meaningful Python code can be
executed immediately, without waiting for the Python interpreter and your
application code to initialize.

This approach is used by the Mercurial version control tool, for example,
where it can shave dozens of milliseconds off of ``hg`` command service
times.

``PyOxidizer`` could potentially support *command servers* as a built-in
feature for *any* Python application.

PyO3
----

`PyO3 <https://github.com/pyo3/pyo3>`_ are alternate Rust bindings to
Python from `rust-cpython <https://github.com/dgrunwald/rust-cpython>`_,
which is what ``pyembed`` currently uses.

The ``PyO3`` bindings seem to be ergonomically better than `rust-cpython`.
``PyOxidizer`` may switch to ``PyO3`` someday. A hard blocker is that
as of at least June 2019, ``PyO3`` requires Nightly Rust. We do not wish
to make Nightly Rust a requirement to run ``PyOxidizer``.
