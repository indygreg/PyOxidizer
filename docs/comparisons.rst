.. comparisons:

==========================
Comparisons to Other Tools
==========================

What makes PyOxidizer different from other Python packaging and distribution
tools? Read on to find out!

PyInstaller
===========

PyInstaller - like ``PyOxidizer`` - can produce a self-contained executable
file containing your application. However, at run-time, PyInstaller will
extract Python source/bytecode files to a temporary directory then import
modules from the filesystem. ``PyOxidizer`` typically skips this step and
loads modules directly from memory.

py2exe
======

`py2exe <http://www.py2exe.org/>`_ is a tool for converting Python scripts
into Windows programs, able to run without requiring an installation.

The goals of py2exe and ``PyOxidizer`` are conceptually very similar.

One major difference between the two is that py2exe works on just Windows
whereas ``PyOxidizer`` works on multiple platforms.

Another significant difference is that py2exe distributes a copy of Python and
your Python resources in a more traditional manner: as a ``pythonXY.dll``
and a ``library.zip`` file containing compiled Python bytecode. ``PyOxidizer``
is able to compile the Python interpreter and your Python resources directly
into the ``.exe`` so there can be as little as a single file providing
your application.

Also, the approach to packaging that py2exe and ``PyOxidizer`` take is
substantially different. py2exe embeds itself into ``setup.py`` as a
``distutils`` extension. ``PyOxidizer`` wants to exist at a higher level
and interact with the output of ``setup.py`` rather than get involved in the
convoluted mess of ``distutils`` internals. This enables ``PyOxidizer`` to
provide value beyond what ``setup.py``/``distutils`` can provide.

Shiv
====

`Shiv <https://shiv.readthedocs.io/en/latest/>`_ is a packager for zip file
based Python applications. The Python interpreter has built-in support for
running self-contained Python applications that are distributed as zip files.

Shiv requires the target system to have a Python executable and for the target
to support shebangs in executable files. This is acceptable for controlled
\*NIX environments. It isn't acceptable for Windows (which doesn't support
shebangs) nor for environments where you can't guarantee an appropriate
Python executable is available.

Also, by distributing our own Python interpreter with the application,
PyOxidizer has stronger guarantees about the run-time environment. For
example, your application can aggressively target the latest Python version.
Another benefit of distributing your own Python interpreter is you can run a
Python interpreter with various optimizations, such as profile-guided
optimization (PGO) and link-time optimization (LTO). You can also easily
configure custom memory allocators or tweak memory allocators for optimal
performance.

PEX
===

`PEX <https://github.com/pantsbuild/pex>`_ is a packager for zip file based
Python applications. For purposes of comparison, PEX and Shiv have the
same properties.

XAR
===

`XAR <https://github.com/facebookincubator/xar/>`_ requires the use of SquashFS.
SquashFS requires Linux.

``PyOxidizer`` is a target native executable and doesn't require any special
filesystems or other properties to run.

Docker / Running a Container
============================

It is increasingly popular to distribute applications as self-contained
container environments. e.g. Docker images. This distribution mechanism
is effective for Linux users.

``PyOxidizer`` will likely produce a smaller distribution than container-based
applications. This is because many container-based applications contain a lot
of extra content that isn't needed by the processes within.

``PyOxidizer`` also doesn't require a container execution environment. Not
every user has the capability to run certain container formats. However,
nearly every user can run a self-contained executable.

At run time, ``PyOxidizer`` executes a native binary and doesn't have to go
through any additional execution layers. Contrast this with Docker, which
uses HTTP requests to create containers, sets up temporary filesystems and
networks for the container, etc. Spawning a process in a Docker container can
take tens of milliseconds or more. This overhead can be prohibitive for low
latency applications like CLI tools.

Nuitka
======

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

PyRun
=====

`PyRun <https://www.egenix.com/products/python/PyRun>`_ can produce single
file executables. The author isn't sure how it works. PyRun doesn't
appear to support modern Python versions. And it appears to require shared
libraries (like bzip2) on the target system. ``PyOxidizer`` supports
the latest Python and doesn't require shared libraries that aren't in
nearly every environment.
