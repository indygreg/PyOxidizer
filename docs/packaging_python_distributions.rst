.. _packaging_python_distributions:

==================================
Understanding Python Distributions
==================================

The :ref:`config_type_python_distribution` Starlark type represents
a Python *distribution*, an entity providing a Python installation
and build files which PyOxidizer uses to build your applications. See
:ref:`config_concept_python_distribution` for more.

.. _packaging_available_python_distributions:

Available Python Distributions
==============================

PyOxidizer ships with its own list of available Python distributions.
These are constructed via the
:ref:`config_default_python_distribution` Starlark method. Under
most circumstances, you'll want to use one of these distributions
instead of providing your own because these distributions are tested
and should have maximum compatibility.

Here are the built-in Python distributions:

+---------+---------+--------------------+--------------+------------+
| Source  | Version | Flavor             | Build Target              |
+=========+=========+====================+===========================+
| CPython |   3.8.6 | standalone_dynamic | x86_64-unknown-linux-gnu  |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.0 | standalone_dynamic | x86_64-unknown-linux-gnu  |
+---------+---------+--------------------+---------------------------+
| CPython |   3.8.6 | standalone_static  | x86_64-unknown-linux-musl |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.0 | standalone_static  | x86_64-unknown-linux-musl |
+---------+---------+--------------------+---------------------------+
| CPython |   3.8.6 | standalone_dynamic | i686-pc-windows-msvc      |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.0 | standalone_dynamic | i686-pc-windows-msvc      |
+---------+---------+--------------------+---------------------------+
| CPython |   3.8.6 | standalone_static  | i686-pc-windows-msvc      |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.0 | standalone_static  | i686-pc-windows-msvc      |
+---------+---------+--------------------+---------------------------+
| CPython |   3.8.6 | standalone_dynamic | x86_64-pc-windows-msvc    |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.0 | standalone_dynamic | x86_64-pc-windows-msvc    |
+---------+---------+--------------------+---------------------------+
| CPython |   3.8.6 | standalone_static  | x86_64-pc-windows-msvc    |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.0 | standalone_static  | x86_64-pc-windows-msvc    |
+---------+---------+--------------------+---------------------------+
| CPython |   3.8.6 | standalone_dynamic | x86_64-apple-darwin       |
+---------+---------+--------------------+---------------------------+
| CPython |   3.9.0 | standalone_dynamic | x86_64-apple-darwin       |
+---------+---------+--------------------+---------------------------+

All of these distributions are provided by the
`python-build-standalone <https://github.com/indygreg/python-build-standalone>`_,
and are maintained by the maintainer of PyOxidizer.

.. _packaging_python_version_compatibility:

Python Version Compatibility
============================

PyOxidizer is capable of working with Python 3.8 and 3.9.

Python 3.8 is the default Python version because it has been around
for a while and is relatively stable. Once Python 3.9 matures, it
will eventually become the default Python version.

PyOxidizer's tests are run primarily against the default Python
version. So adopting a non-default version may risk running into
subtle bugs.

.. _packaging_choosing_python_distribution:

Choosing a Python Distribution
==============================

The Python 3.8 distributions are the default and are better tested
than the Python 3.9 distributions. If you care about stability,
you should use 3.8.

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
* On macOS, distributions are compiled with ``MACOSX_DEPLOYMENT_TARGET=10.9``
  so they only used SDK features present on macOS >=10.9, enabling them to
  run on sufficiently old macOS versions.
