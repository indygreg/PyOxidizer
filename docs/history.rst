.. _history:

===============
Project History
===============

Work on PyOxidizer started in November 2018 by Gregory Szorc.

Blog Posts
==========

* `Building Standalone Python Applications with PyOxidizer <https://gregoryszorc.com/blog/2019/06/24/building-standalone-python-applications-with-pyoxidizer>`_ (2019-06-23)
* `PyOxidizer Support for Windows <https://gregoryszorc.com/blog/2019/01/06/pyoxidizer-support-for-windows>`_ (2019-01-06)
* `Faster In-Memory Python Module Importing <https://gregoryszorc.com/blog/2018/12/28/faster-in-memory-python-module-importing>`_ (2018-12-28)
* `Distributing Standalone Python Applications <https://gregoryszorc.com/blog/2018/12/18/distributing-standalone-python-applications>`_ (2018-12-18)

Version History
===============

0.2
---

**Not yet released**

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

None yet.

New Features
^^^^^^^^^^^^

None yet.

Bug Fixes
^^^^^^^^^

None yet.

All Relevant Changes
^^^^^^^^^^^^^^^^^^^^

None yet.

0.1.1
-----

Released on June 24, 2019.

Bug Fixes
^^^^^^^^^

* ``pyoxidizer`` binaries built from crates should now properly
  refer to an appropriate commit/tag in PyOxidizer's canonical Git
  repository in auto-generated ``Cargo.toml`` files. (#11)

0.1
---

Released on June 24, 2019. This is the initial formal release of PyOxidizer.
The first ``pyoxidizer`` crate was published to ``crates.io``.

New Features
^^^^^^^^^^^^

* Support for building standalone, single file executables embedding Python
  for 64-bit Windows, macOS, and Linux.
* Support for importing Python modules from memory using zero-copy.
* Basic Python packaging support.
* Support for jemalloc as Python's memory allocator.
* ``pyoxidizer`` CLI command with basic support for managing project
  lifecycle.
