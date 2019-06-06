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

TOML configuration files allow extensive customization of packaging and
run time behavior. Many projects can be successfully packaged with
PyOxidizer today.

Major Missing Features
======================

The ``importlib.abc.ResourceReader`` interface is not yet supported. There's
no way to embed non-module *resource* data into binaries. But you can
distribute these resources next to a binary and use traditional filesystem
importers configured via ``sys.path`` to load resources. Support for
``importlib.abc.ResourceReader`` is planned.

Building and using compiled extension modules (e.g. C extensions) is not
yet supported. This is a hard problem on a few dimensions. We have a plan
to solve it, however.

``pyoxidizer add`` and ``pyoxidizer analyze`` aren't fully implemented. There
is no ``pyoxidizer upgrade`` command. Work on all of these is planned.

We don't yet have a good story for the *distributing* part of the application
distribution problem. We're good at producing executables. But we'd like to
go the extra mile and make it easier for people to produce installers, ``.dmg``
files, tarballs, etc. This includes providing build environments for e.g.
non-MUSL based Linux executables. It also includes support for auditing
for license compatibility (e.g. screening for GPL components in proprietary
applications) and assembling required license texts to satisfy notification
requirements in those licenses.

Lesser Missing Features
=======================

Error handling in build-time Rust code isn't great. Expect to see the
``pyoxidizer`` executable to crash from time to time. Crashes in binaries
built with PyOxidizer should not occur and will be treated as serious bugs!

Only Python 3.7 is currently supported. Support for older Python 3
releases is possible. But the project author hopes we only need to
target the latest/greatest Python release.

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

Cross compiling is not yet supported. We hope to and believe we can
support this someday. We would like to eventually get to a state where you
can e.g. produce Windows and macOS executables from Linux. It's possible.

Naming and semantics in the TOML configuration files can be significantly
improved. There's also various missing packaging functionality.
