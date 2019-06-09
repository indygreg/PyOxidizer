.. _distributing:

=====================
Distributing Binaries
=====================

There are a handful of considerations for distributing binaries built
with PyOxidizer.

Foremost, PyOxidizer doesn't yet have a turnkey solution for various
distribution problems. PyOxidizer currently produces a binary
(typically an executable application) and then leaves the final
packaging (like generating installers) up to the user. We eventually
want to tackle some of these problems.

Binary Compatibility
====================

You claim binaries produced with PyOxidizer can run nearly anywhere. Is
that true?

It depends.

On Linux, a binary built with MUSL should *just work* on pretty much any
Linux machine.

If you are linking against ``libc.so``, things get more complicated
because the binary will link against the ``glibc`` symbol versions
that were present on the build machine. To ensure maximum binary
compatibility, compile your binary in a Debian 7 environment, as this will use
a sufficiently old version of glibc which should work in most Linux
environments.

Of course, if you control the execution environment (like if executables
will run on the same machine that built them), then this may not pose a
problem to you. Use the ``pyoxidizer analyze`` command to inspect binaries
for compatibility before attempting to run a binary on a different machine
it was produced on.

.. _licensing_considerations:

Licensing Considerations
========================

Any time you link libraries together or distribute software, you need
to be concerned with the licenses of the underlying code. Some software
licenses - like the GPL - can require that any code linked with them be
subject to the license and therefore be made open source. In addition,
many licenses require a license and/or copyright notice be attached to
works that use or are derived from the project using that license. So
when building or distributing **any** software, you need to be cognizant
about all the software going into the final work and any licensing
terms that apply. Binaries produced with PyOxidizer are no different!

PyOxidizer and the code it uses in produced binaries is licensed under
the Mozilla Public License version 2.0. The licensing terms are
generally pretty favorable. (If the requirements are too strong, the
code that ships with binaries could potentially use a *weaker* license.
Get in touch with the project author.)

The Rust code PyOxidizer produces relies on a handful of 3rd party
Rust crates. These crates have various licenses. We recommend using
the `cargo-license <https://github.com/onur/cargo-license>`_,
`cargo-tree <https://github.com/sfackler/cargo-tree>`_, and
`cargo-lichking <https://github.com/Nemo157/cargo-lichking>`_ tools to
examine the Rust crate dependency tree and their respective licenses.
The ``cargo-lichking`` tool can even assemble licenses of Rust dependencies
automatically so you can more easily distribute those texts with your
application!

As cool as these Rust tools are, they don't include licenses for the
Python distribution, the libraries its extensions link against, nor any
3rd party Python packages you may have packaged.

Python and its various dependencies are governed by a handful of licenses.
These licenses have various requirements and restrictions.

At the very minimum, the binary produced with PyOxidizer will have a
Python distribution which is governed by a license. You will almost certainly
need to distribute a copy of this license with your application.

Various C-based extension modules part of Python's standard library
link against other C libraries. For self-contained Python binaries,
these libraries will be statically linked if they are present. That
can trigger *stronger* license protections. For example, if all
extension modules are present, the produced binary may contain a copy
of the GPL 3.0 licensed ``readline`` and ``gdbm`` libraries, thus triggering
strong copyleft protections in the GPL license.

.. important::

   It is critical to audit which Python extensions and packages are being
   packaged because of licensing requirements of various extensions.

Currently, PyOxidizer doesn't have a good way to audit for license use
in produced binaries. This is definitely something we want to add. We
want to go a step further and have explicit configuration options for
e.g. preventing GPL extensions/libraries from being compiled into binaries,
because accidentally linking a proprietary application with a GPL 3.0
licensed library would have potentially disastrous consequences!

For now, a list of licenses used by CPython and the various libraries
its extensions link against can be found at
https://github.com/indygreg/python-build-standalone/blob/master/python-licenses.rst.
You will likely need to make copies of some of these licenses available
if you distribute any binary produced with PyOxidizer.
