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

Binaries produced with PyOxidizer should be able to run nearly anywhere.
The details and caveats vary depending on the operating system and target
platform and are documented in the sections below.

.. important::

   Please create issues at https://github.com/indygreg/PyOxidizer/issues
   when the content of this section is incomplete or lacks important
   details.

The ``pyoxidizer analyze`` command can be used to analyze the contents
of executables and libraries. For example, for ELF binaries it will list
all shared library dependencies and analyze glibc symbol versions and
print out which Linux distributions it thinks the binary is compatible
with. Please note that ``pyoxidizer analyze`` is not yet implemented on
all platforms.

Windows
-------

Binaries built with PyOxidizer have a run-time dependency on various
DLLs. Most of the DLLs are Windows system DLLs and should always be
installed.

Binaries built with PyOxidizer have a dependency on the Visual Studio
C++ Runtime. You will need to distribute a copy of ``vcruntimeXXX.dll``
alongside your binary or trigger the install of the Visual Studio
C++ Redistributable in your application installer so the dependency is
managed at the system level (the latter is preferred).

There is also currently a dependency on the Universal C Runtime (UCRT).

PyOxidizer will eventually make producing Windows installers from packaged
applications turnkey. Until that time arrives, see the
`Microsoft documentation <https://docs.microsoft.com/en-us/cpp/windows/deploying-native-desktop-applications-visual-cpp?view=vs-2019>`_
on deployment considerations for Windows binaries. The
`Dependency Walker <http://www.dependencywalker.com/>`_ tool is also
useful for analyzing DLL dependencies.

macOS
-----

The Python distributions that PyOxidizer consumers are built with
``MACOSX_DEPLOYMENT_TARGET=10.9``, so they should be compatible with
macOS versions 10.9 and newer.

The Python distribution has dependencies against a handful of system
libraries and frameworks. These frameworks should be present on all
macOS installations.

Linux
-----

On Linux, a binary built with musl libc should *just work* on pretty much
any Linux machine. See :ref:`statically_linked_linux` for more.

If you are linking against ``libc.so``, things get more complicated
because the binary will probably link against the ``glibc`` symbol versions
that were present on the build machine. To ensure maximum binary
compatibility, compile your binary in a Debian 7 environment, as this
will use a sufficiently old version of glibc which should work in most
Linux environments.

Of course, if you control the execution environment (like if executables
will run on the same machine that built them), then this may not pose a
problem to you. Use the ``pyoxidizer analyze`` command to inspect binaries
for compatibility before distributing a binary so you know what the
requirements are.

.. _statically_linked_linux:

Building Fully Statically Linked Binaries on Linux
--------------------------------------------------

It is possible to produce a fully statically linked executable embedding
Python on Linux. The produced binary will have no external library
dependencies nor will it even support loading dynamic libraries. In theory,
the executable can be copied between Linux machines and it will *just work*.

Building such binaries requires using the ``x86_64-unknown-linux-musl``
Rust toolchain target. Using ``pyoxidizer``::

   $ pyoxidizer build --target x86_64-unknown-linux-musl

Specifying ``--target x86_64-unknown-linux-musl`` will cause PyOxidizer
to use a Python distribution built against
`musl libc <https://www.musl-libc.org/>`_ as well as tell Rust to target
*musl on Linux*.

Targeting musl requires that Rust have the musl target installed. Standard
Rust on Linux installs typically do not have this installed! To install it::

   $ rustup target add x86_64-unknown-linux-musl
   info: downloading component 'rust-std' for 'x86_64-unknown-linux-musl'
   info: installing component 'rust-std' for 'x86_64-unknown-linux-musl'

If you don't have the musl target installed, you get a build time error
similar to the following::

   error[E0463]: can't find crate for `std`
     |
     = note: the `x86_64-unknown-linux-musl` target may not be installed

But even installing the target may not be sufficient! The standalone
Python builds are using a modern version of musl and the Rust musl
target must also be using this newer version or else you will see
linking errors due to missing symbols. For example::

    /build/Python-3.7.3/Python/bootstrap_hash.c:132: undefined reference to `getrandom'
    /usr/bin/ld: /build/Python-3.7.3/Python/bootstrap_hash.c:132: undefined reference to `getrandom'
    /usr/bin/ld: /build/Python-3.7.3/Python/bootstrap_hash.c:136: undefined reference to `getrandom'
    /usr/bin/ld: /build/Python-3.7.3/Python/bootstrap_hash.c:136: undefined reference to `getrandom'

Rust 1.37 or newer is required for the modern musl version compatibility.
Rust 1.37 is Rust Nightly until July 4, 2019, at which point it becomes
Rust Beta. It then becomes Rust Stable on August 15, 2019. You may need to
override the Rust toolchain used to build your project so Rust 1.37+ is
used. For example::

   $ rustup override set nightly
   $ rustup target add --toolchain nightly x86_64-unknown-linux-musl

This will tell Rust that the ``nightly`` toolchain should be used for
the current directory and to install musl support for the ``nightly``
toolchain.

Then you can build away::

   $ pyoxidizer build --target x86_64-unknown-linux-musl
   $ ldd build/apps/myapp/x86_64-unknown-linux-musl/debug/myapp
        not a dynamic executable

Congratulations, you've produced a fully statically linked executable containing
a Python application!

.. important::

   There are
   `reported performance problems <https://superuser.com/questions/1219609/why-is-the-alpine-docker-image-over-50-slower-than-the-ubuntu-image>`_
   with Python linked against musl libc. Application maintainers are therefore
   highly encouraged to evaluate potential performance issues before distributing
   binaries linked against musl libc.

   It's worth noting that in the default configuration PyOxidizer binaries
   will use ``jemalloc`` for memory allocations, bypassing musl's apparently
   slower memory allocator implementation. This *may* help mitigate reported
   performance issues.

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

Showing Python Distribution Licenses
------------------------------------

The special Python distributions that PyOxidizer consumes can annotate
licenses of software within.

The ``pyoxidizer python-distribution-licenses`` command can display the
licenses for the Python distribution and libraries it may link against.
This command can be used to evaluate which extensions meet licensing
requirements and what licensing requirements apply if a given extension
or library is used.
