.. py:currentmodule:: starlark_pyoxidizer

.. _packaging_python_distributions:

==================================
Understanding Python Distributions
==================================

The :py:class:`PythonDistribution` Starlark type represents
a Python *distribution*, an entity providing a Python installation
and build files which PyOxidizer uses to build your applications. See
:ref:`config_concept_python_distribution` for more.

.. _packaging_available_python_distributions:

Available Python Distributions
==============================

PyOxidizer ships with its own list of available Python distributions.
These are constructed via the
:py:func:`default_python_distribution` Starlark function. Under
most circumstances, you'll want to use one of these distributions
instead of providing your own because these distributions are tested
and should have maximum compatibility.

Here are the built-in Python distributions:

+---------+---------+--------------------+--------------+------------+
| Source  | Version | Flavor             | Build Target              |
+=========+=========+====================+===========================+
| CPython |  3.8.12 | standalone_dynamic | x86_64-unknown-linux-gnu  |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.7 | standalone_dynamic | x86_64-unknown-linux-gnu  |
+---------+---------+--------------------+---------------------------+
| CPython |  3.8.12 | standalone_static  | x86_64-unknown-linux-musl |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.7 | standalone_static  | x86_64-unknown-linux-musl |
+---------+---------+--------------------+---------------------------+
| CPython |  3.8.12 | standalone_dynamic | i686-pc-windows-msvc      |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.7 | standalone_dynamic | i686-pc-windows-msvc      |
+---------+---------+--------------------+---------------------------+
| CPython |  3.8.12 | standalone_static  | i686-pc-windows-msvc      |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.7 | standalone_static  | i686-pc-windows-msvc      |
+---------+---------+--------------------+---------------------------+
| CPython |  3.8.12 | standalone_dynamic | x86_64-pc-windows-msvc    |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.7 | standalone_dynamic | x86_64-pc-windows-msvc    |
+---------+---------+--------------------+---------------------------+
| CPython |  3.8.12 | standalone_static  | x86_64-pc-windows-msvc    |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.7 | standalone_static  | x86_64-pc-windows-msvc    |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.7 | standalone_dynamic | aarch64-apple-darwin      |
+---------+---------+--------------------+---------------------------+
| CPython |  3.8.12 | standalone_dynamic | x86_64-apple-darwin       |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.7 | standalone_dynamic | x86_64-apple-darwin       |
+---------+---------+--------------------+---------------------------+

All of these distributions are provided by the
`python-build-standalone <https://github.com/indygreg/python-build-standalone>`_,
and are maintained by the maintainer of PyOxidizer.

Here is what those target triple values translate to:

``aarch64-apple-darwin``
   64-bit ARM compiled for macOS.
``i686-pc-windows-msvc``
   32-bit Windows using the Microsoft Visual C++ Compiler.
``x86-64-pc-windows-msvc``
   64-bit Windows using the Microsoft Visual C++ Compiler.
``x86_64-apple-darwin``
   64-bit Intel processors compiled for macOS.
``x86_64-pc-unknown-linux-gnu``
   64-bit x86 (typically Intel or AMD) targeting Linux, with a dependency on
   GNU libc (glibc / ``libc.so``).
``x86_64-pc-unknown-linux-musl``
   64-bit x86 (typically Intel or AMD) targeting Linux using musl libc.
   (Musl libc uses static linking for libc, unlike glibc.)

.. _packaging_python_version_compatibility:

Python Version Compatibility
============================

PyOxidizer is capable of working with Python 3.8 and 3.9.

Python 3.9 is the default Python version because it has been around
for a while and is relatively stable.

PyOxidizer's tests are run primarily against the default Python
version. So adopting a non-default version may risk running into
subtle bugs.

.. _packaging_choosing_python_distribution:

Choosing a Python Distribution
==============================

The Python 3.9 distributions are the default and are better tested
than the Python 3.8 distributions. 3.8 was the default in previous
releases and is known to work.

The ``standalone_dynamic`` distributions behave much more similarly
to traditional Python build configurations than do their
``standalone_static`` counterparts. The ``standalone_dynamic``
distributions are capable of loading Python extension modules that
exist as shared library files. So when working with ``standalone_dynamic``
distributions, Python wheels containing pre-built Python extension
modules often *just work*.

The downside to ``standalone_dynamic`` distributions is that you cannot
produce a single file, statically-linked executable containing your
application in most circumstances: you will need a ``standalone_static``
distribution to produce a single file executable.

But as soon as you encounter a third party extension module with a
``standalone_static`` distribution, you will need to recompile it. And
this is often unreliable.

.. _packaging_python_distribution_portability:

Binary Portability of Distributions
===================================

The built-in Python distributions are built in such a way that they should
run on nearly every system for the platform they target. This means:

* All 3rd party shared libraries are part of the distribution (e.g.
  ``libssl`` and ``libsqlite3``) and don't need to be provided by the
  run-time environment.
* Some distributions are statically linked and have no dependencies on
  any external shared libraries.
* On the glibc linked Linux distributions, they use an old glibc version
  for symbol versions, enabling them to run on Linux distributions created
  years ago. (The current version is 2.19, which was released in 2014.)
* Any shared libraries not provided by the distribution are available in
  base operating system installs. On Linux, example shared libraries include
  ``libc.so.6`` and ``linux-vdso.so.1``, which are part of the Linux Standard
  Base Core Configuration and should be present on all conforming Linux
  distros. On macOS, referenced dylibs include ``libSystem``, which is part
  of the macOS core install.
* For Linux, see :ref:`pyoxidizer_distributing_linux` for portability
  considerations.
* For macOS, see :ref:`pyoxidizer_distributing_macos` for portability
  considerations.
* For Windows, see :ref:`pyoxidizer_distributing_windows` for portability
  considerations.

.. _packaging_python_distribution_knowns_issues:

Known Issues with Distributions
===============================

There are various known issues with various distributions. The
python-build-standalone project documentation at
https://python-build-standalone.readthedocs.io/en/latest/ attempts to capture
many of them.

PyOxidizer contains workaround for many of the limitations. For example,
PyOxidizer (specifically the ``pyembed`` Rust crate) can automatically
configure the terminfo database at run-time.

The ``aarch64-apple-darwin`` Python distributions are considered beta quality
because PyOxidizer does not have continuous CI coverage for this architecture.
Releases should be tested before they are released. But there may be
undetected breakage on unreleased commits on the ``main`` branch due to
lack of CI coverage. This limitation should go away once GitHub Actions
supports running jobs on M1 hardware.
