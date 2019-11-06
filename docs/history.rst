.. _history:

===============
Project History
===============

Work on PyOxidizer started in November 2018 by Gregory Szorc.

Blog Posts
==========

* `C Extension Support in PyOxidizer <https://gregoryszorc.com/blog/2019/06/30/c-extension-support-in-pyoxidizer/>`_ (2019-06-30)
* `Building Standalone Python Applications with PyOxidizer <https://gregoryszorc.com/blog/2019/06/24/building-standalone-python-applications-with-pyoxidizer>`_ (2019-06-24)
* `PyOxidizer Support for Windows <https://gregoryszorc.com/blog/2019/01/06/pyoxidizer-support-for-windows>`_ (2019-01-06)
* `Faster In-Memory Python Module Importing <https://gregoryszorc.com/blog/2018/12/28/faster-in-memory-python-module-importing>`_ (2018-12-28)
* `Distributing Standalone Python Applications <https://gregoryszorc.com/blog/2018/12/18/distributing-standalone-python-applications>`_ (2018-12-18)

Version History
===============

next
----

* The ``stdlib`` packaging rule now supports ``excludes`` option
  that allows ignoring specific modules, especially useful for removing
  unnecessary default Python packages such as distutils, pip and ensurepip.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The minimum Rust version to build has been increased from 1.31 to
  1.36. This is mainly due to requirements from the ``starlark``
  crate. We could potentially reduce the minimum version requirements
  again with minimal changes to 3rd party crates.
* PyOxidizer configuration files are now
  `Starlark <https://github.com/bazelbuild/starlark>`_ instead of TOML
  files. The default file name is ``pyoxidizer.bzl`` instead of
  ``pyoxidizer.toml``. All existing configuration files will need to be
  ported to the new format.

0.4.0
-----

Released October 27, 2019.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The ``setup-py-install`` packaging rule now has its ``package_path``
  evaluated relative to the PyOxidizer config file path rather than the
  current working directory.

Bug Fixes
^^^^^^^^^

* Windows now explicitly requires dynamic linking against ``msvcrt``.
  Previously, this wasn't explicit. And sometimes linking the final
  executable would result in unresolved symbol errors because the Windows
  Python distributions used external linkage of CRT symbols and for some
  reason Cargo wasn't dynamically linking the CRT.
* Read-only files in Python distributions are now made writable to avoid
  future permissions errors (#123).
* In-memory ``InspectLoader.get_source()`` implementation no longer errors
  due to passing a ``memoryview`` to a function that can't handle it (#134).
* In-memory ``ResourceReader`` now properly handles multiple resources (#128).

New Features
^^^^^^^^^^^^

* Added an ``app-path`` command that prints the path to a packaged
  application. This command can be useful for tools calling PyOxidizer,
  as it will emit the path containing the packaged files without forcing
  the caller to parse command output.
* The ``setup-py-install`` packaging rule now has an ``excludes`` option
  that allows ignoring specific packages or modules.
* ``.py`` files installed into app-relative locations now have corresponding
  ``.pyc`` bytecode files written.
* The ``setup-py-install`` packaging rule now has an ``extra_global_arguments``
  option to allow passing additional command line arguments to the ``setup.py``
  invocation.
* Packaging rules that invoke ``pip`` or ``setup.py`` will now set a
  ``PYOXIDIZER=1`` environment variable so Python code knows at packaging
  time whether it is running in the context of PyOxidizer.
* The ``setup-py-install`` packaging rule now has an ``extra_env`` option to
  allow passing additional environment variables to ``setup.py`` invocations.
* ``[[embedded_python_config]]`` now supports a ``sys_frozen`` flag to control
  setting ``sys.frozen = True``.
* ``[[embedded_python_config]]`` now supports a ``sys_meipass`` flag to control
  setting ``sys._MEIPASS = <exe directory>``.
* Default Python distribution upgraded to 3.7.5 (from 3.7.4). Various
  dependency packages also upgraded to latest versions.

All Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^^^^^

* Built extension modules marked as app-relative are now embedded in the
  finaly binary rather than being ignored.

0.3.0
-----

Released on August 16, 2019.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The ``pyembed::PythonConfig`` struct now has an additional
  ``extra_extension_modules`` field.
* The default musl Python distribution now uses LibreSSL instead of
  OpenSSL. This should hopefully be an invisible change.
* Default Python distributions now use CPython 3.7.4 instead of 3.7.3.
* Applications are now built into directories named
  ``apps/<app_name>/<target>/<build_type>`` rather than
  ``apps/<app_name>/<build_type>``. This enables builds for multiple targets
  to coexist in an application's output directory.
* The ``program_name`` field from the ``[[embedded_python_config]]`` config
  section has been removed. At run-time, the current executable's path is
  always used when calling ``Py_SetProgramName()``.
* The format of embedded Python module data has changed. The ``pyembed`` crate
  and ``pyoxidizer`` versions must match exactly or else the ``pyembed`` crate
  will likely crash at run-time when parsing module data.

Bug Fixes
^^^^^^^^^

* The ``libedit`` extension variant for the ``readline`` extension should now
  link on Linux. Before, attempting to link a binary using this extension
  variant would result in missing symbol errors.
* The ``setup-py-install`` ``[[packaging_rule]]`` now performs actions to
  appease ``setuptools``, thus allowing installation of packages using
  ``setuptools`` to (hopefully) work without issue (#70).
* The ``virtualenv`` ``[[packaging_rule]]`` now properly finds the
  ``site-packages`` directory on Windows (#83).
* The ``filter-include`` ``[[packaging_rule]]`` no longer requires both
  ``files`` and ``glob_files`` be defined (#88).
* ``import ctypes`` now works on Windows (#61).
* The in-memory module importer now implements ``get_resource_reader()`` instead
  of ``get_resource_loader()``. (The CPython documentation steered us in the
  wrong direction - https://bugs.python.org/issue37459.)
* The in-memory module importer now correctly populates ``__package__`` in
  more cases than it did previously. Before, whether a module was a package
  was derived from the presence of a ``foo.bar`` module. Now, a module will be
  identified as a package if the file providing it is named ``__init__``. This
  more closely matches the behavior of Python's filesystem based importer. (#53)

New Features
^^^^^^^^^^^^

* The default Python distributions have been updated. Archives are generally
  about half the size from before. Tcl/tk is included in the Linux and macOS
  distributions (but PyOxidizer doesn't yet package the Tcl files).
* Extra extension modules can now be registered with ``PythonConfig`` instances.
  This can be useful for having the application embedding Python provide its
  own extension modules without having to go through Python build mechanisms
  to integrate those extension modules into the Python executable parts.
* Built applications now have the ability to detect and use ``terminfo``
  databases on the execution machine. This allows applications to interact
  with terminals properly. (e.g. the backspace key will now work in interactive
  ``pdb`` sessions). By default, applications on non-Windows platforms will
  look for ``terminfo`` databases at well-known locations and attempt to load
  them.
* Default Python distributions now use CPython 3.7.4 instead of 3.7.3.
* A warning is now emitted when a Python source file contains ``__file__``. This
  should help trace down modules using ``__file__``.
* Added 32-bit Windows distribution.
* New ``pyoxidizer distribution`` command for producing distributable artifacts
  of applications. Currently supports building tar archives and ``.msi`` and
  ``.exe`` installers using the WiX Toolset.
* Libraries required by C extensions are now passed into the linker as
  library dependencies. This should allow C extensions linked against
  libraries to be embedded into produced executables.
* ``pyoxidizer --verbose`` will now pass verbose to invoked ``pip`` and
  ``setup.py`` scripts. This can help debug what Python packaging tools are
  doing.

All Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^^^^^

* The list of modules being added by the Python standard library is
  no longer printed during rule execution unless ``--verbose`` is used.
  The output was excessive and usually not very informative.

0.2.0
-----

Released on June 30, 2019.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
* Applications are now built into an ``apps/<appname>/(debug|release)``
  directory instead of ``apps/<appname>``. This allows debug and release
  builds to exist side-by-side.

Bug Fixes
^^^^^^^^^

* Extracted ``.egg`` directories in Python package directories should now have
  their resources detected properly and not as Python packages with the name
  ``*.egg``.
* ``site-packages`` directories are now recognized as Python resource package
  roots and no longer have their contents packaged under a ``site-packages``
  Python package.

New Features
^^^^^^^^^^^^

* Support for building and embedding C extensions on Windows, Linux, and macOS
  in many circumstances. See :ref:`status_extension_modules` for support status.
* ``pyoxidizer init`` now accepts a ``--pip-install`` option to pre-configure
  generated ``pyoxidizer.toml`` files with packages to install via ``pip``.
  Combined with the ``--python-code`` option, it is now possible to create
  ``pyoxidizer.toml`` files for a ready-to-use Python application!
* ``pyoxidizer`` now accepts a ``--verbose`` flag to make operations more
  verbose. Various low-level output is no longer printed by default and
  requires ``--verbose`` to see.

All Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^^^^^

* Packaging now automatically creates empty modules for missing parent
  packages. This prevents a module from being packaged without its parent.
  This could occur with *namespace packages*, for example.
* ``pip-install-simple`` rule now passes ``--no-binary :all:`` to pip.
* Cargo packages updated to latest versions.

0.1.3
-----

Released on June 29, 2019.

Bug Fixes
^^^^^^^^^

* Fix Python refcounting bug involving call to ``PyImport_AddModule()`` when
  ``mode = module`` evaluation mode is used. The bug would likely lead to
  a segfault when destroying the Python interpreter. (#31)
* Various functionality will no longer fail when running ``pyoxidizer`` from
  a Git repository that isn't the canonical ``PyOxidizer`` repository. (#34)

New Features
^^^^^^^^^^^^

* ``pyoxidizer init`` now accepts a ``--python-code`` option to control which
  Python code is evaluated in the produced executable. This can be used to
  create applications that do not run a Python REPL by default.
* ``pip-install-simple`` packaging rule now supports ``excludes`` for excluding
  resources from packaging. (#21)
* ``pip-install-simple`` packaging rule now supports ``extra_args`` for adding
  parameters to the pip install command. (#42)

All Relevant Changes
^^^^^^^^^^^^^^^^^^^^

* Minimum Rust version decreased to 1.31 (the first Rust 2018 release). (#24)
* Added CI powered by Azure Pipelines. (#45)
* Comments in auto-generated ``pyoxidizer.toml`` have been tweaked to
  improve understanding. (#29)

0.1.2
-----

Released on June 25, 2019.

Bug Fixes
^^^^^^^^^

* Honor ``HTTP_PROXY`` and ``HTTPS_PROXY`` environment variables when
  downloading Python distributions. (#15)
* Handle BOM when compiling Python source files to bytecode. (#13)

All Relevant Changes
^^^^^^^^^^^^^^^^^^^^

* ``pyoxidizer`` now verifies the minimum Rust version meets requirements
  before building.

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
