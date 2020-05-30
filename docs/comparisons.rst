.. _comparisons:

==========================
Comparisons to Other Tools
==========================

What makes ``PyOxidizer`` different from other Python packaging and distribution
tools? Read on to find out!

If you are curious why PyOxidizer's creator felt the need to create a
new tool, see
:ref:`faq_why_another_tool` in the FAQ.

.. important::

   It is important for Python application maintainers to make informed
   decisions about their use of packaging tools. If you feel the comparisons
   in this document are incomplete or unfair, please
   `file an issue <https://github.com/indygreg/PyOxidizer/issues>`_ so
   this page can be improved.

.. _compare_pyinstaller:

PyInstaller
===========

`PyInstaller <https://www.pyinstaller.org/>`_ is a tool to convert regular
python scripts to "standalone" executables. The standard packaging produces
a tiny executable and a custom directory structure to host dynamic libraries
and Python code (zipped compiled bytecode).
``PyInstaller`` can produce a self-contained executable file containing your
application, however, at run-time, PyInstaller will extract binary
files and a custom `ZlibArchive <https://pyinstaller.readthedocs.io/en/latest/advanced-topics.html#zlibarchive>`_
to a temporary directory then import modules from the filesystem.
``PyOxidizer`` typically skips this step and loads modules directly from
memory using zero-copy. This makes ``PyOxidizer`` executables significantly
faster to start.

Currently a big difference is that ``PyOxidizer`` needs to build all the binary
dependencies from scratch to facilitate linking into single file,
``PyInstaller`` can work with normal Python packages with a complex system of
hooks to find the runtime dependencies, this allow a lot of not easy to build
packages like PyQt to work out of the box.

.. _compare_py2exe:

py2exe
======

`py2exe <http://www.py2exe.org/>`_ is a tool for converting Python scripts
into Windows programs, able to run without requiring an installation.

The goals of ``py2exe`` and ``PyOxidizer`` are conceptually very similar.

One major difference between the two is that ``py2exe`` works on just Windows
whereas ``PyOxidizer`` works on multiple platforms.

One trick that ``py2exe`` employs is that it can load ``libpython`` and
Python extension modules (which are actually dynamic link libraries) and
other libraries from memory - not filesystem files. They employ a
`really clever hack <https://sourceforge.net/p/py2exe/svn/HEAD/tree/trunk/py2exe/source/README-MemoryModule.txt>`_
to do this! This is similar in nature to what Google does internally with
a custom build of glibc providing a
`dlopen_from_offset() <https://sourceware.org/bugzilla/show_bug.cgi?id=11767>`_.
Essentially, ``py2exe`` embeds DLLs and other entities as *resources*
in the PE file (the binary executable format for Windows) and is capable
of loading them from memory. This allows ``py2exe`` to run things from a
single binary, just like ``PyOxidizer``! The main difference is ``py2exe``
relies on clever DLL loading tricks rather than ``PyOxidizer``'s approach
of using custom builds of Python (which exist as a single binary/library)
to facilitate this. This is a really clever solution and ``py2exe``'s
authors deserve commendation for pulling this off!

The approach to packaging that ``py2exe`` and ``PyOxidizer`` take is
substantially different. py2exe embeds itself into ``setup.py`` as a
``distutils`` extension. ``PyOxidizer`` wants to exist at a higher level
and interact with the output of ``setup.py`` rather than get involved in the
convoluted mess of ``distutils`` internals. This enables ``PyOxidizer`` to
provide value beyond what ``setup.py``/``distutils`` can provide.

``py2exe`` is a mature Python packaging/distribution tool for Windows. It
offers a lot of similar functionality to ``PyOxidizer``.

.. _compare_py2app:

py2app
======

`py2app <https://py2app.readthedocs.io/en/latest/>`_ is a setuptools
command which will allow you to make standalone application bundles
and plugins from Python scripts.

``py2app`` only works on macOS. This makes it like a macOS version of
``py2exe``. Most :ref:`comparisons to py2exe <compare_py2exe>` are
analogous for ``py2app``.

.. _compare_cx_freeze:

cx_Freeze
=========

`cx_Freeze <https://cx-freeze.readthedocs.io/en/latest/>`_ is a set of
scripts and modules for freezing Python scripts into executables.

The goals of ``cx_Freeze`` and ``PyOxidizer`` are conceptually very
similar.

Like other tools in the *produce executables* space, ``cx_Freeze`` packages
Python traditionally. On Windows, this entails shipping a ``pythonXY.dll``.
``cx_Freeze`` will also package dependent libraries found by binaries you
are shipping. This introduces portability problems, especially on Linux.

``PyOxidizer`` uses custom Python distributions that are built in such
a way that they are highly portable across machines. ``PyOxidizer`` can
also produce single file executables.

.. _compare_shiv:

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

.. _compare_pex:

PEX
===

`PEX <https://github.com/pantsbuild/pex>`_ is a packager for zip file based
Python applications. For purposes of comparison, PEX and Shiv have the
same properties. See :ref:`compare_shiv` for this comparison.

.. _compare_xar:

XAR
===

`XAR <https://github.com/facebookincubator/xar/>`_ requires the use of SquashFS.
SquashFS requires Linux.

``PyOxidizer`` is a target native executable and doesn't require any special
filesystems or other properties to run.

.. _compare_docker:

Docker / Running a Container
============================

It is increasingly popular to distribute applications as self-contained
container environments. e.g. Docker images. This distribution mechanism
is effective for Linux users.

``PyOxidizer`` will almost certainly produce a smaller distribution than
container-based applications. This is because many container-based applications
contain a lot of extra content that isn't needed by the processes within.

``PyOxidizer`` also doesn't require a container execution environment. Not
every user has the capability to run certain container formats. However,
nearly every user can run an executable.

At run time, ``PyOxidizer`` executes a native binary and doesn't have to go
through any additional execution layers. Contrast this with Docker, which
uses HTTP requests to create containers, set up temporary filesystems and
networks for the container, etc. Spawning a process in a new Docker
container can take hundreds of milliseconds or more. This overhead can be
prohibitive for low latency applications like CLI tools. This overhead
does not exist for ``PyOxidizer`` executables.

.. _compare_nuitka:

Nuitka
======

`Nuitka <http://nuitka.net/pages/overview.html>`_ can compile Python programs
to single executables. And the emphasis is on *compile*: Nuitka actually
converts Python to C and compiles that. Nuitka is effectively an alternate
Python interpreter.

Nuitka is a cool project and purports to produce significant speed-ups
compared to CPython!

Since Nuitka is effectively a new Python interpreter, there are risks to
running Python in this environment. Some code has dependencies on CPython
behaviors. There may be subtle bugs are lacking features from Nuitka.
However, Nuitka supposedly supports every Python construct, so many
applications should *just work*.

Given the performance benefits of Nuitka, it is a compelling alternative
to ``PyOxidizer``.

.. _compare_pyrun:

PyRun
=====

`PyRun <https://www.egenix.com/products/python/PyRun>`_ can produce single
file executables. The author isn't sure how it works. PyRun doesn't
appear to support modern Python versions. And it appears to require shared
libraries (like bzip2) on the target system. ``PyOxidizer`` supports
the latest Python and doesn't require shared libraries that aren't in
nearly every environment.

.. _compare_pynsist:

pynsist
=======

`pynsist <https://pynsist.readthedocs.io/en/latest/index.html>`_ is a
tool for building Windows installers for Python applications. pynsist
is very similar in spirit to PyOxidizer.

A major difference between the projects is that pynsist focuses on
solving the application distribution problem on Windows where ``PyOxidizer``
aims to solve larger problems around Python application distribution, such
as performance optimization (via loading Python modules from memory
instead of the filesystem).

``PyOxidizer`` has yet to invest significantly into making producing
distributable artifacts (such as Windows installers) simple, so pynsist
still has an advantage over ``PyOxidizer`` here.

.. _compare_bazel:

Bazel
=====

Bazel has `Python rules <https://docs.bazel.build/versions/master/be/python.html>`_
for building Python binaries and libraries. From a high level, it works
similarly to how PyOxidizer's Starlark config files allow you to perform
much of the same actions.

The executables produced by ``py_binary`` are significantly different
from what PyOxidizer does, however.

An executable produced by ``py_binary`` is a glorified self-executing
zip file. At run time, it extracts Python resources to a temporary
directory and then runs a Python interpreter against them. The approach
is similar in nature to what Shiv and PEX do.

PyOxidizer, by contrast, produces a specialized binary containing the
Python interpreter and allows you to embed Python resources inside that
binary, enabling Python modules to be imported without the overhead of
writing a temporary directory and extracting a zip file.
