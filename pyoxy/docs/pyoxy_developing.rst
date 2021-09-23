.. _pyoxy_developing:

=================
Development Guide
=================

Development of PyOxy strives to look like a normal Rust crate as much as
possible. This means that normal workflows such as ``cargo build`` and
``cargo test`` should work.

Please note that if you are working from the root of the PyOxidizer Git
repository, you may want to limit the package being operated on via e.g.
``cargo build -p pyoxy`` or ``cargo test -p pyoxy``.

The ``pyoxy`` crate depends on ``pyembed``, which depends on ``pyo3``, which
insists on finding a runnable and linkable Python install in ``PATH``. You
can set the ``PYO3_PYTHON`` environment variable to point at an explicit
Python interpreter rather than utilizing the default search logic.

Building ``pyoxy`` With Embedded Python
=======================================

If you just ``cargo build``, it is likely that ``pyo3`` will pick up a
non-portable, dynamically linked ``libpython``. Furthermore, it will load
Python resources from the filesystem, from the path configured in ``libpython``.
Such a configuration is not portable across machines!

To produce a portable, single file executable embedding ``libpython`` and
its resources, we need to perform a little extra work. This essentially entails
asking PyOxidizer to produce a static ``libpython``, a Python packed resources
file containing the standard library, and a PyO3 configuration file. Then, we
build with a reference to that PyO3 configuration file to link the appropriate
``libpython`` and pick up the packed resources file.

.. code-block::

   cd pyoxy
   pyoxidizer build --release
   PYO3_CONFIG_FILE=$(pwd)/build/x86_64-unknown-linux-gnu/release/resources/pyo3-build-config-file.txt cargo build --release

On Linux, to ensure portability of the produced binary, you'll need to link
against a sufficiently old (and therefore widely available) glibc version.
This often entails running the build/link in an older Linux distribution,
such as Debian Jessie.

On macOS, you'll probably want to define ``MACOSX_DEPLOYMENT_TARGET`` to the
minimum version of macOS you want to target or else the produced binary won't
be portable.
