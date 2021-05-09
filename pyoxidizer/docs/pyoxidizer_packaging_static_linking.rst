.. py:currentmodule:: starlark_pyoxidizer

.. _packaging_static_linking:

=========================================================
Standalone / Single File Applications with Static Linking
=========================================================

This document describes how to produce standalone, single file application
binaries embedding Python using static linking.

See also :ref:`packaging_extension_modules` for extensive documentation
about extension modules, which are often a pain point when it comes to
static linking.

.. _statically_linked_linux:

Building Fully Statically Linked Binaries on Linux
==================================================

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
And newer versions of Rust may change which version of musl they use,
introducing failures similar to above. If you run into problems with a
modern version of Rust, consider
`reporting an issue <https://github.com/indygreg/PyOxidizer/issues>`_ against
PyOxidizer!

Once Rust's musl target is installed, you can build away::

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

.. _statically_linked_windows:

Building Statically Linked Binaries on Windows
==============================================

It is possibly to produce a mostly self-contained ``.exe`` on Windows.
We say *mostly* self-contained here because currently the built binary
has some external ``.dll`` dependencies. However, these DLLs are core
Windows / system DLLs and should be present on any Windows installation
supported by the Python distribution being used.

The main trick to build a statically linked Windows binary is to
switch the Python distribution from the default ``standalone_dynamic``
flavor to ``standalone_static``. This can be done via the following in
your config file:

.. code-block:: python

    dist = default_python_distribution(flavor = "standalone_static")

.. important::

   The ``standalone_static`` Windows distributions build Python in a way that
   is incompatible with compiled Python extensions (``.pyd`` files). So if you
   use this distribution flavor, you will need to compile all Python extensions
   from source and cannot use pre-built wheels packages. This can make building
   applications with many dependencies difficult, as many Python packages don't
   compile on Windows without installing many dependencies first.

   See also :ref:`packaging_extension_modules_windows_static`.

See also :ref:`packaging_python_distributions` for more details on the
differences between ``standalone_dynamic`` and ``standalone_static`` Python
distributions.

Implications of Static Linking
==============================

Most Python distributions rely heavily on dynamic linking. In addition to
``python`` frequently loading a dynamic ``libpython``, many C extensions
are compiled as standalone shared libraries. This includes the modules
``_ctypes``, ``_json``, ``_sqlite3``, ``_ssl``, and ``_uuid``, which
provide the native code interfaces for the respective non-``_`` prefixed
modules which you may be familiar with.

These C extensions frequently link to other libraries, such as ``libffi``,
``libsqlite3``, ``libssl``, and ``libcrypto``. And more often than not,
that linking is dynamic. And the libraries being linked to are provided
by the system/environment Python runs in. As a concrete example, on
Linux, the ``_ssl`` module can be provided by
``_ssl.cpython-37m-x86_64-linux-gnu.so``, which can have a shared library
dependency against ``libssl.so.1.1`` and ``libcrypto.so.1.1``, which
can be located in ``/usr/lib/x86_64-linux-gnu`` or a similar location
under ``/usr``.

When Python extensions are statically linked into a binary, the Python
extension code is part of the binary instead of in a standalone file.

If the extension code is linked against a static library, then the code
for that dependency library is part of the extension/binary instead of
dynamically loaded from a standalone file.

When ``PyOxidizer`` produces a fully statically linked binary, the code
for these 3rd party libraries is part of the produced binary and not
loaded from external files at load/import time.

There are a few important implications to this.

One is related to security and bug fixes. When 3rd party libraries are
provided by an external source (typically the operating system) and are
dynamically loaded, once the external library is updated, your binary
can use the latest version of the code. When that external library is
statically linked, you need to rebuild your binary to pick up the latest
version of that 3rd party library. So if e.g. there is an important
security update to OpenSSL, you would need to ship a new version of your
application with the new OpenSSL in order for users of your application
to be secure. This shifts the security onus from e.g. your operating
system vendor to you. This is less than ideal because security updates
are one of those problems that tend to benefit from greater centralization,
not less.

It's worth noting that PyOxidizer's library security story is very similar
to that of containers (e.g. Docker images). If you are OK distributing and
running Docker images, you should be OK with distributing executables
built with PyOxidizer.

Another implication of static linking is licensing considerations. Static
linking can trigger stronger licensing protections and requirements.
Read more at :ref:`licensing_considerations`.
