.. _pyoxy_installing:

==========
Installing
==========

It is **highly recommended** to obtain and use one of the official pre-built
binaries for PyOxy. These can be obtained from GitHub releases. Go to
https://github.com/indygreg/PyOxidizer/releases and scroll until you find
the latest release for PyOxy.

System Requirements
===================

The requirements in this section only apply to the official pre-built
binaries. Binaries built by others may not have the same requirements.

Linux
-----

The x86_64-unknown-linux-gnu binaries should work on any Linux having
glibc 2.18+ and GCC 4.2+ and conform to the Linux Standard Base Core
Specification. This should be ~every Linux distribution released since
2014-2015.

.. note::

   Modern versions of Fedora / CentOS / RHEL have a bug and
   `don't conform to the LSB Core Specification <https://bugzilla.redhat.com/show_bug.cgi?id=2055953>`_
   unless you install the ``libxcrypt-compat`` package. If you see
   an error about missing ``libcrypt.so.1``, your distribution is buggy.

macOS
-----

The x86_64-apple-darwin binaries target macOS 10.9+.

The aarch64-apple-darwin binaries target macOS 11.0+.

The binaries should work on a fresh install of macOS.
