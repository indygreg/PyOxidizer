.. py:currentmodule:: starlark_pyoxidizer

.. _history:

===============
Project History
===============

Work on PyOxidizer started in November 2018 by Gregory Szorc.

Blog Posts
==========

* `Announcing the 0.9 Release of PyOxidizer <https://gregoryszorc.com/blog/2020/10/18/announcing-the-0.9-release-of-pyoxidizer/>`_ (2020-10-18)
* `Announcing the 0.8 Release of PyOxidizer <https://gregoryszorc.com/blog/2020/10/12/announcing-the-0.8-release-of-pyoxidizer/>`_ (2020-10-12)
* `Using Rust to Power Python Importing with oxidized_importer <https://gregoryszorc.com/blog/2020/05/10/using-rust-to-power-python-importing-with-oxidized_importer/>`_ (2020-05-10)
* `PyOxidizer 0.7 <https://gregoryszorc.com/blog/2020/04/09/pyoxidizer-0.7/>`_ (2020-04-09)
* `C Extension Support in PyOxidizer <https://gregoryszorc.com/blog/2019/06/30/c-extension-support-in-pyoxidizer/>`_ (2019-06-30)
* `Building Standalone Python Applications with PyOxidizer <https://gregoryszorc.com/blog/2019/06/24/building-standalone-python-applications-with-pyoxidizer>`_ (2019-06-24)
* `PyOxidizer Support for Windows <https://gregoryszorc.com/blog/2019/01/06/pyoxidizer-support-for-windows>`_ (2019-01-06)
* `Faster In-Memory Python Module Importing <https://gregoryszorc.com/blog/2018/12/28/faster-in-memory-python-module-importing>`_ (2018-12-28)
* `Distributing Standalone Python Applications <https://gregoryszorc.com/blog/2018/12/18/distributing-standalone-python-applications>`_ (2018-12-18)

.. _version_history:

Version History
===============

.. _version_0_22_0:

0.23.0
------

(Not yet released.)

Bug Fixes
^^^^^^^^^

* Default macOS Python distributions should no longer crash when running
  tkinter. This fixes a regression introduced in the 0.20 release.

Changes
^^^^^^^

* Default CPython distributions upgraded. CPython 3.10.4 upgraded to 3.10.5.
  See https://github.com/indygreg/python-build-standalone/releases/tag/20220630
  for additional changes.

0.22.0
------

Released June 5, 2022.

Bug Fixes
^^^^^^^^^

* macOS binaries no longer dynamically link ``liblzma.5.dylib``.

.. _version_0_21_0:

0.21.0
------

Released June 4, 2022.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The minimum Rust version has been changed from 1.56 to 1.60 to facilitate
  use of features required by some Rust crates.
* The default Python version is 3.10 (instead of 3.9).

Bug Fixes
^^^^^^^^^

* Fixed regression in the behavior of various ``pyoxidizer`` commands which
  prevented them from working without specifying any arguments. This regression
  was introduced in 0.20 with the upgrade of the ``clap`` crate to version 3.1.
  (#523)
* PyO3 Rust crates upgraded from 0.16.4 to 0.16.5. The upgrade fixes compatibility
  issues with Python 3.10 that could lead to runtime crashes or incorrect behavior
  in many configurations.
* Fixed a runtime panic when incorrect attribute assignments were attempted on the
  ``PythonExtensionModule``, ``PythonPackageDistributionResource``, and
  ``PythonPackageResource`` Starlark types. (#561)
* We no longer panic when encountering invalid UTF-8 when reading process output
  of various ``python`` invocations. Previously, invocations of ``pip``,
  ``setup.py``, and other processes could result in a panic if invalid UTF-8 was
  emitted. (#579)

New Features
^^^^^^^^^^^^

* Default CPython distributions upgraded from 3.8.12, 3.9.10, and 3.10.2
  to 3.8.13, 3.9.13, and 3.10.4, respectively. See additional changes in
  these distributions at
  https://github.com/indygreg/python-build-standalone/releases/tag/20220318,
  https://github.com/indygreg/python-build-standalone/releases/tag/20220501,
  and https://github.com/indygreg/python-build-standalone/releases/tag/20220528.
* The default Python version is now 3.10 (instead of 3.9).
* The mechanism for handling software licenses has been overhauled.

  * The formatting of licenses during building has changed significantly.
  * Rust licensing information is now dynamically derived at build time rather
    than derived from a static list. The Rust components with annotated licensing
    should be more accurate as a result.
  * :py:class:`PythonExecutable` Starlark types now write out a file containing
    licensing information for software components within the binary. This restores
    a feature that was dropped in version 0.5. The name of the file (or disabling
    of the feature) can be controlled via the
    :py:attr:`PythonExecutable.licenses_filename` attribute.
  * A new ``pyoxidizer rust-project-licensing`` command for printing licensing
    information for Rust projects. This can be used to help debug Rust licensing
    issues or to generate licensing content for any Rust project.
  * A :py:meth:`PythonExecutable.add_cargo_manifest_licensing` Starlark method for
    registering the licensing information for a ``Cargo.toml`` Rust project. This can
    be used by Rust projects wishing to have their licensing information captured.
* Initial support for ``aarch64-unknown-linux-gnu`` Python distributions. The
  distributions are now defined and PyOxidizer should use them when appropriate.
  However, the distributions aren't yet well tested. So feedback on their
  operation via GitHub issues would be appreciated!
* ``aarch64-apple-darwin`` (Apple M1) now has a default Python 3.8 distribution.
  This distribution is not very well tested and use of a newer distribution is
  strongly preferred.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* Managed Rust toolchain upgraded from 1.56.1 to 1.61.0.
* Starlark :py:class:`PythonInterpreterConfig` documentation has been changed
  to refer to :ref:`pyembed_interpreter_config`. The latter is automatically
  derived from the canonical Rust source code. So the change effectively results
  in a single, consistent set of documentation for interpreter configuration.
* The mechanism for locating the Apple SDK now uses the
  `apple-sdk <https://crates.io/crates/apple-sdk>`_ Rust crate. The new crate
  work similarly to how our custom logic was working before. But there may be
  subtle changes in behavior. If you see new build errors related to Apple SDKs
  in this release, don't hesitate to create an issue. One notable change is that
  we will now look for SDKs in all ``/Applications/Xcode*.app`` directories. In
  environments like GitHub Actions, this will result in finding and using the
  newest installed SDK.
* The jemalloc allocator in built binaries has been upgraded to version 5.3.
* The auto-generated Rust project created during binary building is now explicitly
  licensed to the public domain.
* Derivation of a custom ``libpython`` library archive now sometimes uses pure
  Rust code instead of calling external processes. There should be no meaningful
  change in behavior except for build output being more concise.
* Auto-generated Rust projects now contain an empty ``[workspace]`` table in their
  ``Cargo.toml``. This enables auto-generated projects to be nested under an existing
  workspace directory without Cargo complaining. This approach is more robust in
  the common case where the Rust project isn't part of a larger workspace.

.. _version_0_20_0:

0.20.0
------

Released March 6, 2022.

Bug Fixes
^^^^^^^^^

* The ``pyembed`` crate will now properly call
  ``multiprocessing.spawn.spawn_main()`` when the ``multiprocessing`` auto
  dispatch function as configured by
  :py:attr:`PythonInterpreterConfig.multiprocessing_start_method` is set to
  ``spawn``. This resolves a ``TypeError: spawn_main() missing 1 required
  positional argument: 'pipe_handle'`` run-time error that would occur in this
  configuration. (#483)
* The ``pyembed`` crate better handles errors during interpreter initialization.
  This fixes a regression to the error handling introduced by the port to PyO3
  in version 0.18.0. (#481)
* The ``pyembed::MainPythonInterpreter`` type is now more resilient against
  calling into a finalized Python interpreter. Before, calling ``py_runmain()``
  (possibly via ``run()``) could result in a segfault in the type's ``Drop``
  implementation.
* ``oxidized_importer.OxidizedFinder.find_distributions()`` now properly
  normalizes names when performing comparisons. Previously, the specified
  ``name`` was properly normalized but it was compared against un-normalized
  strings. Both the search and candidate names are now normalized when performing
  a comparison. This should fix cases where case and other special character
  differences could result in a distribution not being found. (#488)
* A potential crash when importing extension modules from memory on Windows was
  fixed. The crash could occur due to discrepancy in Python reference counting when
  multi-phase initialization was used. (#490)
* Our patched ``distutils`` only sets ``Py_BUILD_CORE_BUILTIN`` on Windows. This
  fixes errors building at least the ``grpcio`` package outside of Windows.
* When using a modified ``distutils`` to install Python packages, the
  ``SETUPTOOLS_USE_DISTUTILS=stdlib`` environment variable is now set. This
  prevents ``setuptools`` from using its vendored copy of ``distutils`` and
  ignoring our modifications. Before this change, packages with extension
  modules may not have built correctly, resulting in build or run-time errors.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The ``pyembed::MainPythonInterpreter`` Rust API for controlling embedded
  Python interpreters has been refactored. Various methods now take
  ``&self`` instead of ``&mut self``. ``acquire_gil()`` and ``release_gil()``
  have been removed (use ``with_gil()`` instead). ``MainPythonInterpreter``
  instances now release the GIL after initialization. Before, the GIL would be
  perpetually held by the instance. Consumers that were calling
  ``PyEval_SaveThread()`` to release the GIL to work around this should delete
  calls to that function, as the GIL is now released automatically. APIs on
  ``MainPythonInterpreter`` will acquire the GIL as necessary. (#500)

New Features
^^^^^^^^^^^^

* Support for Python 3.10 on all previously supported platforms. Python 3.9 is
  still the default Python version. Target Python 3.10 by passing
  ``python_version = "3.10"`` to the :py:func:`default_python_distribution`
  Starlark function.
* Default Python distributions upgraded from 3.9.7 to 3.9.10. Various library
  dependencies have also been upgraded. See
  https://github.com/indygreg/python-build-standalone/releases/tag/20211017 and
  https://github.com/indygreg/python-build-standalone/releases/tag/20220222 for
  the full list of changes.
* The ``pyembed::MainPythonInterpreter`` Rust struct has gained a
  ``with_gil()`` function for executing a function with the Python GIL held.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* PyO3 Rust crate upgraded from version ``0.14`` to ``0.16``.
* Managed Rust toolchain upgraded from 1.56.0 to 1.56.1.

.. _version_0_19_0:

0.19.0
------

Released October 28, 2021.

Changes
^^^^^^^

* ``p12`` Rust crate updated to avoid dependency on version yanked from
  crates.io (version 0.18.0 could not be installed via ``cargo`` in some
  configurations because of this).
* PyOxidizer's documentation is now more isolated from the rest of the
  projects in the same repository.

.. _version_0_18_0:

0.18.0
------

Released October 24, 2021.

Bug Fixes
^^^^^^^^^

* The ``unable to identify deployment target environment variable for macosx (please
  report this bug)`` error message seen when attempting to use a too-old macOS SDK
  has been replaced to automatically assume the use of ``MACOSX_DEPLOYMENT_TARGET``.
  A warning message that this will possibly lead to build failures is printed. (#414)
* Invocation of ``signtool.exe`` on Windows now always passes ``/fd SHA256`` by
  default. Previously, we did not specify ``/fd`` unless a signing algorithm was
  explicitly requested. Newer versions of ``signtool.exe`` appear to insist that
  ``/fd`` be specified.
* Default Python distributions now properly advertise system library dependencies
  on Linux and macOS. The older distributions failed to annotate some library
  dependencies, which could lead to missing symbol errors in some build
  environments.
* Linux default Python distributions no longer utilize the ``pthread_yield()``
  function, enabling them to be linked against glibc 2.34+, which deprecated
  the symbol. (#463)
* Python ``.whl`` resources parsing now ignores directories. Previously,
  directories may have been emitted as 0-sized resources.
* In some ``pyoxidizer.bzl`` configurations, an error would occur due to attempting
  to write a built executable to a directory that doesn't exist. This should no
  longer occur. (#447)

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The minimum Rust version has been changed from 1.52 to 1.56 to facilitate
  use of the newest versions of some Rust crates, Rust 2021 edition, and
  some Cargo features to enhance linker control.
* The run-time Rust code for interfacing with the Python interpreter now uses
  the `PyO3 <https://github.com/PyO3/pyo3>`_ crate instead of
  `cpython <https://github.com/dgrunwald/rust-cpython>`_. The code port was quite
  extensive and while we believe all important run-time functionality is
  backwards compatible, there are possibly subtle differences in behavior. Please
  file GitHub issues to report any undesired changes in behavior.
* Development workflows relying on specifying the ``PYTHON_SYS_EXECUTABLE``
  environment variable have changed to use ``PYO3_PYTHON``, as the environment
  variable has changed between the ``cpython`` and ``pyo3`` crates.
* The ``pyembed`` crate no longer has ``cpython-link-unresolved-static`` and
  ``cpython-link-default`` Cargo features. Autogenerated Rust projects also no
  longer have ``cpython-link-unresolved-static`` and ``cpython-link-default``
  features (which existed as proxies to these features in the ``pyembed``
  crate).
* The ``pyoxidizer add`` command has been removed because it didn't work as
  advertised.
* The ``pyembed`` crate no longer has ``build-mode-*`` features and its build
  script no longer attempts to integrate with PyOxidizer or its build artifacts.
* The ``pyembed`` crate no longer annotates a ``links`` entry.
* The mechanism by which auto-generated Rust projects integrate with the
  ``pyembed`` crate has changed substantially. If you had created a standalone
  Rust project via ``pyoxidizer init-rust-project``, you may wish to create a
  fresh project and reconcile differences in the auto-generated files to ensure
  things now build as expected.
* Default Python distributions on macOS aarch64 are now built with macOS SDK
  11.3. macOS x86_64 are now built with macOS SDK 11.1.

New Features
^^^^^^^^^^^^

* Default Python distributions upgraded from 3.8.11 and 3.9.6 to 3.8.12 and
  3.9.7. Various library dependencies have also been upgraded. See
  https://github.com/indygreg/python-build-standalone/releases/tag/20211012 and
  https://github.com/indygreg/python-build-standalone/releases/tag/20211017 for
  the full list of changes.
* When in verbose mode, messages will be printed displaying the actual result
  of every request to add a resource. Before, the Starlark code would emit a
  message like ``adding extension module foo`` before requesting the addition
  and the operation could no-op. This behavior was misleading and hard to debug
  since it often implied a resource was added when in fact it wasn't! The new
  behavior is for the resource collector to tell its callers exactly what
  actions it took and for the actual results to be displayed to the end-user.
  This should hopefully make it easier to debug issues with how resources are
  added to binaries.
* A new ``pyoxidizer generate-python-embedding-artifacts`` command that writes
  out file artifacts useful for embedding Python in another project. The command
  essentially enables other projects to use PyOxidizer's embeddable Python
  distributions without using PyOxidizer to build them. See
  :ref:`pyoxidizer_rust_generic_embedding` for documentation.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* When Apple SDKs are found via the ``SDKROOT`` environment variables, a hard
  error now occurs if that SDK does not support the target platform or deployment
  target. Previously, we would allow the use of the SDK, only to likely encounter
  a hard-to-debug compile error. If the SDKs version does not meet the desired
  minimum version, a warning message is printed but the build proceeds. (#431)
* The ``pyembed`` crate (which built binaries use to interface with an embedded
  Python interpreter) now uses the ``pyo3`` crate instead of ``cpython`` to
  interface with Python APIs.
* Nightly Cargo features are no longer required on Windows. (Courtesy of PyO3
  giving us complete control over how Python is linked.)
* The mechanism by which built binaries link against ``libpython`` has been
  significantly refactored. Before, the ``cpython`` crate would link against
  a partial ``libpython`` in many configurations and the ``pyembed`` crate
  would *complete* the linking with a library defined by PyOxidizer. With the
  PyO3 crate supporting a configuration file to configure all attributes of
  linking, the PyO3 crate now fully links against ``libpython`` and ``pyembed``
  doesn't care about linking ``libpython`` at all.
* The ``pyembed`` crate is now generic and no longer attempts to integrate with
  ``pyoxidizer`` or its build artifacts. The crate can now be used by any Rust
  application wishing to embed a Python interpreter.
* The ``oxidized_importer`` Python extension has been extracted from the
  ``pyembed`` crate and is now defined in the ``python-oxidized-importer``
  crate. The ``pyembed`` crate now depends on this crate to provide the
  custom importer functionality.
* Previous versions of PyOxidizer would not build on Rust 1.56+ due to
  incompatibilities with an older version of the ``starlark`` crate. The crate
  was upgraded to version 0.3.2 to fix this issue.
* Managed Rust toolchain upgraded from 1.54.0 to 1.56.0.

.. _version_0_17_0:

0.17.0
------

Released August 8, 2021.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The minimum Rust version has been changed from 1.46 to 1.52 to facilitate
  use of const generics and some other stabilized APIs.
* :py:meth:`starlark_tugger.PythonWheelBuilder.write_to_directory` now interprets
  relative paths as relative to the currently configured build path, not relative
  to the process's current working directory.
* Various Starlark types now ensure that they cannot get out of sync when
  cloned. Previously, various Starlark types would clone their underlying Rust
  struct when the Starlark value was cloned. This could cause Starlark value
  instances to become out of sync with each other if one value was mutated. Now,
  all mutable Starlark types should hold a reference to a shared resource,
  ensuring that cloned Starlark values all refer to the same instance. This
  change could result in Starlark configuration files behaving differently. For
  example, before you could mutate a value in a function call and that mutation
  wouldn't be reflected in the caller's Starlark value. Now, it would be.
* :py:class:`oxidized_importer.OxidizedFinder` will now automatically call
  :py:func:`multiprocessing.set_start_method` when it imports the
  :py:mod:`multiprocessing` module. Applications that explicitly call
  :py:func:`multiprocessing.set_start_method` may fail with
  ``RuntimeError("context has already been set")`` as a result of this change.
  See :ref:`pyoxidizer_packaging_multiprocessing` for workarounds.
* :py:attr:`PythonInterpreterConfig.sys_frozen` now defaults to ``True``
  instead of ``False``.
* :py:class:`starlark_tugger.WiXInstaller`,
  :py:class:`starlark_tugger.WiXMSIBuilder`, and
  :py:class:`starlark_tugger.WiXBundleBuilder` instances now always default to
  building an installer for the ``x64`` WiX architecture. Previously, the
  default architecture would be derived from the architecture of the running
  binary.
* :py:class:`starlark_tugger.WiXMSIBuilder` instances no longer have a
  ``target_triple`` attribute.

Bug Fixes
^^^^^^^^^

* The default target triple is now derived from the target triple of the
  running binary, not the environment the running binary was built in. In
  many cases these would be identical. However, they would diverge if the
  binary was cross-compiled.
* The default Python packaging policy now disables bytecode generation for
  various modules in the Python standard library in the ``lib2to3.tests`` and
  ``test`` packages that contain invalid Python 3 source code and would fail
  to compile to bytecode. This should enable Python resources to compile without
  error when setting :py:attr:`PythonPackagingPolicy.include_test` to ``True``,
  without requiring a custom resource handling callback to disable bytecode
  generation. (#147)
* Applications with hyphens (``-``) in their name now build properly on Windows.
  Previously, there would be a cryptic build failure when running ``rc.exe``.
  (#402)
* The ELF (read: Linux) binaries in the default Python distributions have
  changed how they perform dynamic library loading so they should always pick
  up the libpython from the distribution. Before, ``LD_LIBRARY_PATH``
  environment variables could result in the wrong libpython being loaded and
  errors like ``ModuleNotFoundError: No module named '_posixsubprocess'`` being
  encountered. (#406)

New Features
^^^^^^^^^^^^

* Default Python distributions upgraded from 3.8.10 and 3.9.5 to 3.8.11 and
  3.9.6. Various library dependencies have also been upgraded. See
  https://github.com/indygreg/python-build-standalone/releases/tag/20210724 for
  the full list of changes.
* :py:class:`oxidized_importer.OxidizedFinder` now calls
  :py:func:`multiprocessing.set_start_method` when the :py:mod:`multiprocessing`
  module is imported. The behavior of this feature can be controlled via the
  new :py:attr:`PythonInterpreterConfig.multiprocessing_start_method` attribute.
  On macOS, the default start method is effectively switched from ``spawn`` to
  ``fork``, as PyOxidizer supports this mode. The main execution routine of
  built executables also now recognizes the *signatures* of processes spawned
  for :py:mod:`multiprocessing` use and will automatically function accordingly.
  This behavior can be disabled via
  :py:attr:`PythonInterpreterConfig.multiprocessing_auto_dispatch`. These changes
  mean that :py:mod:`multiprocessing` should *just work* when default settings are
  used. See :ref:`pyoxidizer_packaging_multiprocessing` for full documentation of
  :py:mod:`multiprocessing` interactions with PyOxidizer.
* The :py:attr:`oxidized_importer.OxidizedFinder.pkg_resources_import_auto_register`
  now exposes whether the :py:class:`oxidized_importer.OxidizedFinder` instance will
  automatically register itself with ``pkg_resources``.
* :py:class:`starlark_tugger.AppleUniversalBinary` has gained the
  :py:meth:`starlark_tugger.AppleUniversalBinary.write_to_directory` method.
* :py:class:`starlark_tugger.FileContent` has gained the
  :py:meth:`starlark_tugger.FileContent.write_to_directory` method.
* :py:class:`starlark_tugger.MacOsApplicationBundleBuilder` has gained the
  :py:meth:`starlark_tugger.MacOsApplicationBundleBuilder.write_to_directory`
  method.
* :py:class:`starlark_tugger.WiXInstaller` has gained the
  :py:meth:`starlark_tugger.WiXInstaller.to_file_content` and
  :py:meth:`starlark_tugger.WiXInstaller.write_to_directory` methods.
* :py:class:`starlark_tugger.WiXMSIBuilder` has gained the
  :py:meth:`starlark_tugger.WiXMSIBuilder.to_file_content` and
  :py:meth:`starlark_tugger.WiXMSIBuilder.write_to_directory` methods.
* :py:class:`starlark_tugger.WiXBundleBuilder` has gained the
  :py:meth:`starlark_tugger.WiXBundleBuilder.to_file_content` and
  :py:meth:`starlark_tugger.WiXBundleBuilder.write_to_directory` methods.
* :py:class:`starlark_tugger.WiXInstaller` has gained the
  :py:attr:`starlark_tugger.WiXInstaller.arch` attribute to retrieve and modify the
  architecture of the WiX installer being built.
* The constructors for :py:class:`starlark_tugger.WiXInstaller`,
  :py:class:`starlark_tugger.WiXMSIBuilder`, and
  :py:class:`starlark_tugger.WiXBundleBuilder` now accept an ``arch`` argument to
  control the WiX architecture of the installer.
* :py:class:`starlark_tugger.WiXMSIBuilder` has gained the
  :py:attr:`starlark_tugger.WiXMSIBuilder.arch` attribute to define the
  architecture of the WiX installer being built.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* Managed Rust toolchain upgraded from 1.52.0 to 1.54.0.
* Visual C++ Redistributable installers upgraded from version 14.28.29910 to
  14.29.30040.

.. _version_0_16_0:

0.16.0
------

Released May 9, 2021.

Bug Fixes
^^^^^^^^^

* The Rust build environment now always sets ``RUSTC`` to the path to the
  Rust compiler that we've detected. This should hopefully prevent
  ``could not execute process `rustc...`` errors in environments where Rust
  is not otherwise installed.
* Pre-release ``pyoxidizer`` binaries built in CI should now generate
  ``Cargo.lock`` files in Rust projects that work with ``cargo build --frozen``.
* Managed Rust toolchains now properly install the Rust stdlib for cross-compiles.
  Previously, the logs said it was installing them but didn't actually, leading
  to build failures due to an incomplete Rust toolchain.
* The file modified times in files extracted from Python distributions are now set
  to the current time. Previously, we preserved the mtime in the tar archive and
  the Windows archives had an mtime of the UNIX epoch. This could lead to runtime
  errors in ``pip`` due to pip attempting to create a zip file of itself and
  Python's zip file code not supporting times older than 1980. If you see a
  ``ValueError: ZIP does not support timestamps before 1980`` error when running
  ``pip`` as part of running PyOxidizer, you are hitting this bug. You will need
  *modernize* the mtimes in the extracted Python distributions. The easiest way to
  do this is to clear PyOxidizer's Python distribution cache via
  ``pyoxidizer cache-clear``.
* MSI installers built with :py:class:`starlark_tugger.WiXMSIBuilder` should now
  properly update the ``PATH`` environment variable if that installation option
  is active. This affects PyOxidizer's own MSI installers.

New Features
^^^^^^^^^^^^

* The new :py:class:`starlark_tugger.PythonWheelBuilder` type can be used to
  create Python wheel (``.whl``) files. It is currently rather low-level and
  doesn't have any integrations with other Starlark Python types. But it does
  allow you to create Python wheels from file content. PyOxidizer uses the
  type for building its own wheels (previously it was using ``maturin``).

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* When building for Apple platforms, we now check for a compatible Apple SDK earlier
  during binary building (when compiling a custom ``config.c`` for a custom
  ``libpython``). This should surface missing dependencies sooner in the build
  and potentially replace cryptic compiler error messages with an actionable one
  about the Apple SDK. Related to this, we now target a specific Apple SDK when
  compiling the aforementioned source file to ensure that the same, validated SDK
  is consistently used.

.. _version_0_15_0:

0.15.0
------

Released May 6, 2021.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The order of the ``content`` and ``path`` arguments to
  :py:meth:`starlark_tugger.MacOsApplicationBundleBuilder.add_macos_file` and
  :py:meth:`starlark_tugger.MacOsApplicationBundleBuilder.add_resources_file` has been reversed
  and ``path`` now defaults to ``None``. While technically a backwards
  incompatible change, the old methods weren't usable in prior versions
  of PyOxidizer because the :py:class:`starlark_tugger.FileContent` Starlark
  type couldn't be instantiated!
* :py:class:`starlark_tugger.FileManifest` now performs path normalization and
  checking on every insertion. Before, there were a few code paths that may have
  skipped this step, causing *bad* paths to be inserted.
* Tracked paths in :py:class:`starlark_tugger.FileManifest` should now have
  Windows-style directory separators (``\``) normalized to UNIX style (``/``).


Bug Fixes
^^^^^^^^^

* Apple code signatures using a time-stamp server now validate Apple's code
  signature checks. Previously, they failed validation due the time-stamped
  data being incorrect.
* The WiX XML IDs and GUIDs in autogenerated ``.wxs`` files corresponding to
  *install files* were sometimes internally inconsistent or duplicated, leading
  to malformed ``.wxs`` files being generated. Autogenerated ``.wxs`` files
  should now hopefully be well-formed.
* Release artifacts should now reference the ``pyembed`` crate from the
  package registry instead of a Git URL. Previously, auto-generated Rust
  projects might insist the ``pyembed`` crate was available at a Git URL.
  This would disagree with the auto-generated ``Cargo.lock`` file and result
  in a build failure due to building with ``cargo build --frozen``.

New Features
^^^^^^^^^^^^

* Default Python distributions upgraded from 3.8.9 and 3.9.4 to 3.8.10 and
  3.9.5.
* PyOxidizer releases are now published as pre-built binary wheels to PyPI and
  can be installed via ``pip install pyoxidizer``.
* Apple code signatures now include a time-stamp token issued by Apple's
  time-stamp server by default. Presence of the time-stamp token in code
  signatures is a requirement to notarize applications.
* It is now possible to add code signatures to Mach-O binaries that don't
  have an existing signature. Previously, it was only possible to sign
  binaries that had an existing signature.
* The :py:class:`starlark_tugger.FileContent` Starlark type can now be
  constructed from filesystem paths or string content via
  :py:meth:`starlark_tugger.FileContent.__init__`. The type also exposes
  mutable attributes :py:attr:`starlark_tugger.FileContent.executable` and
  :py:attr:`starlark_tugger.FileContent.filename` to view and change instance
  state.
* The new :py:meth:`starlark_tugger.FileManifest.add_file` method can be used
  to add a :py:class:`starlark_tugger.FileContent` to a
  :py:class:`starlark_tugger.FileManifest`. The method allows controlling
  the destination path within the :py:class:`starlark_tugger.FileManifest`.
  Combined with the introduction of :py:meth:`starlark_tugger.FileContent.__init__`,
  it is now possible to add arbitrary file-based or string-based files
  to a :py:class:`starlark_tugger.FileManifest`.
* The new :py:meth:`starlark_tugger.FileManifest.paths` method can be used
  to retrieve the paths currently tracked by a
  :py:class:`starlark_tugger.FileManifest`.
* The new :py:meth:`starlark_tugger.FileManifest.get_file` method can be
  used to retrieve a :py:class:`starlark_tugger.FileContent` from a path in
  :py:class:`starlark_tugger.FileManifest`.
  The new :py:meth:`starlark_tugger.FileManifest.remove` method can be used
  to remove a tracked path from a :py:class:`starlark_tugger.FileManifest`.
  The new methods unlock the ability to mutate the contents of
  :py:class:`starlark_tugger.FileManifest` instances.
* Starlark now has a :py:class:`starlark_tugger.AppleUniversalBinary` type
  that can be used to construct *universal*/*fat*/*multi-architecture* Mach-O
  binaries, the binary executable format used by Apple operating systems.
  Starlark primitives like :py:class:`PythonExecutable` can today only yield
  a single architecture binary. However, with the new type, it is possible
  to take multiple source binaries and combine them into a *universal* binary,
  all from Starlark.
* The :py:class:`starlark_tugger.WiXInstaller` Starlark type now exposes mutable
  attributes :py:attr:`starlark_tugger.WiXInstaller.install_files_root_directory_id`
  and :py:attr:`starlark_tugger.WiXInstaller.install_files_wxs_path` to control
  the autogenerated ``.wxs`` file containing fragment for *install files*. See the
  type's documentation for more.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* :py:meth:`starlark_tugger.WiXInstaller.build()` now automatically materializes
  and builds a ``.wxs`` file containing fragments for files registered for
  installation. Before, this Starlark type was not very usable without this file,
  as WiX wouldn't pick up files that had been registered for install.
* Rust 1.52.0 is now used as the default Rust toolchain (from version 1.51.0).
* The musl libc linked default Python distributions no longer use the
  ``reallocarray()`` symbol, which was introduced in musl libc 1.2.2. This
  should enable musl libc builds to work with musl 1.2.1 and possibly older
  versions.

.. _version_0_14_1:

0.14.1
------

Released April 30, 2021.

Bug Fixes
^^^^^^^^^

* Fixed a bug in the 0.14.0 release where newly created projects won't build
  due to ``Cargo.lock`` issues.

.. _version_0_14_0:

0.14.0
------

Released April 30, 2021.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* PyOxidizer no longer uses the system's installed Rust toolchain when
  building projects. By default, it will download and use a specific version
  of the Rust toolchain. See :ref:`pyoxidizer_managed_rust` for instructions
  on disabling this behavior.
* The ``pyembed`` crate now always canonicalizes the path to the current
  executable. Previously, if ``OxidizedPythonInterpreterConfig.exe`` were
  set, it would not be canonicalized. It is possible this could break
  use cases where the current executable is deleted after the executable
  starts. In this case, the Python interpreter will fail to initialize. If
  this functionality is important to you, file a feature request.
* The ``pyembed`` crate will now remove entries from ``sys.path_hooks``
  related to filesystem importers if filesystem importing is disabled.
  Previously, only ``sys.meta_path`` would have its filesystem importers
  removed.
* The ``pyembed`` crate now always registers the
  :py:class:`oxidized_importer.OxidizedFinder` path hook on ``sys.path_hooks``
  when an instance is being installed on ``sys.meta_path``. This ensures that
  consumers of ``sys.path_hooks`` outside the module importing mechanism (such
  as ``pkgutil`` and ``pkg_resources``) can use the path hook.
* The ``pyembed`` crate now registers the
  :py:class:`oxidized_importer.OxidizedFinder` path hook as the 1st entry on
  ``sys.path_hooks``, not the last.
* The :py:class:`oxidized_importer.OxidizedFinder` path hook is now more strict
  about the path values it will respond to. Previously, it would accept ``str``,
  ``bytes``, ``pathlib.Path``, or any other path-like type. Now, it only
  responds to ``str`` values. Furthermore, it will only respond to values that
  exactly match :py:attr:`oxidized_importer.OxidizedFinder.path_hook_base_str` or
  a well-formed virtual sub-directory thereof. Previously, it would attempt to
  canonicalize path strings, taking into account the current working directory,
  filesystem links, and other factors affecting path normalization. The new
  implementation is simpler and by being stricter should be less brittle at
  run-time. See :ref:`oxidized_finder_path_hooks` for documentation on the path
  hooks behavior.
* The ``pyembed`` crate has prefixed all its allocator features (``jemalloc``,
  ``mimalloc``, and ``snmalloc``) with ``allocator-``. This makes the names
  consistent with the features in auto-generated Rust projects.

Bug Fixes
^^^^^^^^^

* Rust projects created with ``pyoxidizer init-rust-project`` no longer fail to
  build due to a cryptic ``writing packed resources`` error.
* When materializing Python package distribution resources (i.e. files in
  ``.dist-info`` and ``.egg-info`` directories) to the filesystem, package names
  are now normalized to lowercase with hyphens replaced with underscores. The new
  behavior matches expectations of official Python resource handling APIs like
  ``importlib.metadata``. Before, APIs like ``importlib.metadata`` would fail
  to find files materialized by PyOxidizer for package names containing a hyphen
  or capital latter. (#394)

New Features
^^^^^^^^^^^^

* PyOxidizer now automatically downloads and uses a Rust toolchain at run time.
  This means there is no longer an install requirement of having Rust already
  available on your system (unless you install PyOxidizer from source). See
  :ref:`pyoxidizer_managed_rust` for details of the new feature, including
  directions on how to disable the feature and have PyOxidizer use an already
  installed Rust.
* :py:class:`oxidized_importer.OxidizedFinder` now supports ``pkg_resources``
  integration. Most of the ``pkg_resources`` APIs are implemented, enabling
  most ``pkg_resources`` functionality to work. ``pkg_resources`` integration
  is automatically enabled upon import of the ``pkg_resources`` module, so
  ``pkg_resources`` integration should *just work* for many applications.
  See :ref:`oxidized_finder_pkg_resources` for the full documentation, including
  which features aren't implemented.
* :py:class:`oxidized_importer.OxidizedFinder` now exposes the properties
  :py:attr:`oxidized_importer.OxidizedFinder.path_hook_base_str` and
  :py:attr:`oxidized_importer.OxidizedFinder.origin`.
* Starlark configuration files can now produce macOS Application Bundles.
  See :py:class`starlark_tugger.MacOsApplicationBundleBuilder` for the API
  documentation.
* ``pyoxidizer`` commands that evaluate Starlark files now accept the arguments
  ``--var`` and ``--var-env`` to define extra variables to define in the
  evaluated Starlark file. This enables Starlark files to be parameterized based
  on explicit strings provided via ``--var`` or through the content of
  environment variables via ``--var-env``.
* PyOxidizer can now automatically add cryptographic code signatures when
  running. This feature is extensively documented at :ref:`tugger_code_signing`.
  From a high-level, you instantiate and activate a
  :py:class:`starlark_tugger.CodeSigner` in your Starlark configuration to
  define your code signing certificate. As files are processed as part of
  evaluating your Starlark configuration file, they are examined for the
  ability to be signed and code signing is automatically attempted. We support
  signing Windows files using Microsoft's official ``signtool.exe``
  application and Apple Mach-O and bundle files using a pure Rust
  reimplementation of Apple's code signing functionality. This functionality
  is still in its early stages of development and is lacking some power user
  features to exert low-level control over code signing. Please file feature
  requests as you encounter limitations with the functionality!
* The new Starlark functions :py:func:`starlark_tugger.prompt_confirm`,
  :py:func:`starlark_tugger.prompt_input`,
  :py:func:`starlark_tugger.prompt_password`,
  and :py:func:`starlark_tugger.can_prompt` can be used to allow configuration
  files to perform interaction with the user via the terminal. The functions all
  allow a default value to be provided, enabling them to be used in scenarios
  when stdin isn't connected to a TTY and can't be prompted.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* The Python API for the ``oxidized_importer`` Python extension module
  providing our custom importer logic is now centrally documented instead of
  spread out over multiple documentation pages. See
  :ref:`oxidized_importer_api_reference` for the new docs. Various type
  references throughout the generated documentation should now link to the
  new API docs.
* The Starlark dialect is now documented as native Python classes and functions
  using Sphinx's support for doing so. The documentation should now look more
  familiar to Python developers familiar with Sphinx for Python API
  documentation.
* PyOxidizer now stores persistent artifacts (like Rust toolchains) and
  downloaded Python distributions) in a per-user *cache* directory. See
  :ref:`pyoxidizer_cache` for more.
* The ``pyoxidizer`` CLI now accepts ``--verbose`` as a sub-command argument.
  Previously, it was only accepted as an argument before the sub-command name.
* Generated Rust projects (which can be temporary as part of building binaries)
  now contain a ``Cargo.lock`` file and are built with ``cargo build --locked``.
  The template of the ``Cargo.lock`` is static and under version control. The
  presence of the ``Cargo.lock`` coupled with ``cargo build --locked`` should
  ensure that Rust crate versions used by Rust projects exactly match those used
  by the build of PyOxidizer that produced the project. This should result
  in more deterministic builds and higher reliability of build success.

.. _version_0_13_2:

0.13.2
------

Released April 15, 2021.

Bug Fixes
^^^^^^^^^

* Fixes a build failure on Windows.

.. _version_0_13_1:

0.13.1
------

Released April 15, 2021.

Bug Fixes
^^^^^^^^^

* The 0.13.0 release contained improper crate paths in ``Cargo.toml`` files
  due to a bug in our automated release mechanism. This release should fix
  those issues.

.. _version_0_13_0:

0.13.0
------

Released April 15, 2021.

Bug Fixes
^^^^^^^^^

* ``WiXSimpleMsiBuilder`` now properly writes XML when a license file is provided.
* ``WixBundleInstallerBuilder`` now handles the *already installed* exit code from
  the VC++ Redistributable installer as a success condition. Previously, installs
  would abort.
* ``WixBundleInstallerBuilder`` no longer errors on a missing build directory
  when attempting to download the Visual C++ Redistributable runtime files.

New Features
^^^^^^^^^^^^

* Per-platform Windows MSI and multi-platform Windows exe installers for
  PyOxidizer are now available. The installers are built with PyOxidizer,
  using its built-in support for producing Windows installers.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* Default CPython distributions upgraded from 3.9.3 to 3.9.4.
* Default Python distributions upgraded setuptools from 54.2.0 to 56.0.0.

.. _version_0_12_0:

0.12.0
------

Released April 14, 2021.

.. danger::

   The 0.12.0 release uses CPython 3.9.3, which inadvertently shipped an ABI
   incompatible change, causing some extension modules to not work or crash.
   Please avoid this release if you use pre-built Python extension modules.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The minimum Rust version has been changed from 1.45 to 1.46 to facilitate
  use of `const fn`.
* On Apple platforms, PyOxidizer now validates that the Apple SDK being used
  is compatible with the Python distribution being used and will abort the
  build if not. Previously, PyOxidizer would blindly use whatever SDK was
  the default and this could lead to cryptic error messages when building
  (likely undefined symbol errors when linking). The current default Python
  distributions impose a requirement of the macosx10.15+ SDK for Python 3.8 and
  macosx11.0+ for Python 3.9. See issue #373 for a comprehensive discussion
  of this topic.
* On Apple platforms, binaries built with PyOxidizer now automatically target
  the OS version that the Python distribution was built to target. Previously,
  binaries would likely target the OS version of the building machine unless
  explicit action was taken. The practical effect of this change is binaries
  targeting x86_64 should now work on macOS 10.9 without any end-user action
  required.
* Documentation URLs for PyOxidizer now all consistently begin with
  ``pyoxidizer_``. Many old documentation URLs no longer work.

Bug Fixes
^^^^^^^^^

* The autogenerated ``pyoxidizer.bzl`` correctly references the ``no-copyleft``
  extension module filter instead of the old ``no-gpl`` value.
* Linux binaries using the ``libedit`` variant of the ``readline`` Python
  extension (occurs when using the ``no-copyleft`` extension module filter)
  no longer encounter an undefined symbol error when linking. (#376)
* The `ctypes` extension was previously compiled incorrectly, leading to
  run-time errors on various platforms. These issues should be fixed.

New Features
^^^^^^^^^^^^

* On Apple platforms, PyOxidizer now automatically locates, validates, and
  uses an appropriate SDK given the settings of the Python distribution being
  used. PyOxidizer will reject building with an SDK older than the one used
  to produce the Python distribution. PyOxidizer will automatically use the
  newest installed SDK compatible with the target configuration. The SDK
  and targeting information is printed during builds. See
  :ref:`pyoxidizer_distributing_macos_build_machine_requirements` for details
  on how to override default behavior.
* ``OxidizedFinder`` now implements ``path_hook()`` and a path hook is
  automatically registered on ``sys.path_hooks`` during interpreter
  initialization when an ``OxidizedFinder`` is being used. Feature
  contributed by William Schwartz in #343.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* The ``snmalloc`` allocator now uses the C API directly and avoids going
  through an allocation tracking layer, improving the performance of this
  allocator. Improvement contributed by Ryan Clanton.
* Python distributions updated to latest versions. Changes include:
  macOS Python 3.8 is now built against the 10.15 SDK instead of 11.1;
  musl libc upgraded to 1.2.2; setuptools upgraded to 54.2.0; LibreSSL upgraded
  to 3.2.5; OpenSSL upgraded to 1.1.1k; SQLite upgraded to 3.35.4.

.. _version_0_11_0:

0.11.0
------

Released March 4, 2021.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The default Python distribution is now CPython 3.9 instead of 3.8. To use
  3.8, pass the ``python_version="3.8"`` argument to
  :py:func:`default_python_distribution` in your configuration file. We
  don't anticipate dropping support for 3.8 any time soon. However, this may
  be necessary in order to more easily support new Python features.
* The Python 3.8 distributions no longer support Windows 7 and require Windows
  8, Windows 2012, or newer. The Python 3.9 distributions already required these
  Windows versions.
* The minimum Rust version has been changed from 1.41 to 1.45 to facilitate
  the use of procedural macros.
* The ``pyembed::MainPythonInterpreter::run_as_main()`` method has been renamed
  to ``py_runmain()`` to reflect that it always calls ``Py_RunMain()``.
* The ``py-module-names`` file is no longer written as part of the files
  comprising an embedded Python interpreter.
* ``OxidizedFinder.__init__()`` no longer accepts ``resources_data`` and
  ``resources_file`` argument to specify the resources to load. Instead, call one
  of the new ``index_*`` methods on constructed instances.
* ``OxidizedFinder.__init__()`` no longer automatically indexes builtin
  extension modules and frozen modules. Instead, you must now call one of the
  ``index_*`` methods to index these resources.
* The ``pyembed::OxidizedPythonInterpreterConfig.packed_resources`` field is now
  a ``Vec<pyembed::PackedResourcesSource>`` instead of ``Vec<&[u8]>``. The new
  enum allows specifying files as alternative resources sources.
* The ``no-gpl`` value of ``PythonPackagingPolicy.extension_module_filter``
  has been changed to ``no-copyleft`` and it operates on the SPDX license
  annotations instead of a list we maintained.
* ``show_alloc_count`` has been removed from types representing Python
  interpreter configuration because support for this feature was removed in
  Python 3.9.
* ``pyembed::MainPythonInterpreter.acquire_gil()``'s signature has changed, now
  returning a ``Python`` value directly without wrapping it in a ``Result``.
* ``pyembed::OxidizedPythonInterpreterConfig`` had its memory allocator fields
  refactored to support new features and to help prevent bad configs (like
  defining multiple custom memory allocators).
* The Starlark ``PythonInterpreterConfig.raw_allocator`` field has been renamed
  to ``allocator_backend``. The ``system`` value has been renamed to
  ``default``.
* The ``pyembed`` crate now canonicalizes the current executable's path
  and uses this canonicalized path when resolving values with ``$ORIGIN``
  in them. Previously, the path passed into the program was used without
  resolving symlinks, etc. If that path were a symlink or hardlink,
  unexpected results could ensue.
* ``OxidizedFinder.find_distributions()`` now returns an iterator of
  ``OxidizedDistribution`` instead of a ``list``. Code in the standard
  library of older versions of CPython expected an iterator to be returned
  and the new behavior is more compatible. This change enables
  ``importlib.metadata.metadata()`` to work with ``OxidizedFinder``.

Bug Fixes
^^^^^^^^^

* Escaping of string and path values when emitting Rust code for the embedded
  Python interpreter configuration should now be more robust. Previously,
  special characters (like ``\``) were not escaped properly. (#321)
* The ``load()`` Starlark function should now work. (#328)
* ``pyembed::OxidizedPythonInterpreterConfig.argv`` is now always used when
  set, even if ``self.interpreter_config.argv`` is also set.
* ``OxidizedFinder`` now normalizes trailing ``.__init__`` in module names
  to be equivalent to the parent package to partially emulate CPython's
  behavior. See :ref:`oxidized_importer_dunder_init_module_names` for more.
  (#317)
* The lifetime of ``pyembed::MainPythonInterpreter.acquire_gil()``'s return
  value has been adjusted so the Rust compiler will refuse to compile code
  that could crash due to attempting to use a finalized interpreter. (#345)
* ``pyembed::MainPythonInterpreter.py_runmain()``'s signature has changed, now
  consuming ownership of the receiver. Subsequent borrows of ``self`` now fail
  to compile rather than causing runtime errors.
* The optional ``rust`` memory allocator is now thread-safe. Previously, it
  wasn't and releasing of the GIL could lead to memory corruption and crashes.
* ``OxidizedResourceCollector.oxidize()`` should now properly clean up the
  temporary directory it uses during execution. Before, premature Python
  interpreter termination (such as during failing tests) could cause the
  temporary directory to not be removed. Closes #346. Fix contributed by
  William Schwartz in #347.
* ``OxidizedFinder.find_distributions()`` now properly handles the default/empty
  ``Context`` instance (specifically instances where ``.name = None``).
  Previously, ``name = None`` would filter as if ``.name = "None"``. This
  means that all distributions should now be returned with the default/empty
  ``Context`` instance.
* ``OxidizedFinder.find_distributions()`` now properly filters when the
  passed ``Context``'s ``name`` attribute is set to a string. Previously,
  the ``name`` and ``path`` attributes had their order swapped in a function
  call, leading to incorrect filtering.
* The Windows ``standalone_static`` distributions should now work again. They
  had been broken for a few releases and likely never worked with Python 3.9.
  Test coverage of this build configuration has been added to help prevent
  future regressions. (#360)

New Features
^^^^^^^^^^^^

* Support added for ``aarch64-apple-darwin`` (Apple M1 machines). Only Python
  3.9 is supported on this architecture. Because we do not have CI coverage
  for this architecture (due to GitHub Actions not yet having M1 machines),
  support is considered beta quality at this time.
* The ``FileManifest`` Starlark type now exposes an ``add_path()`` to add a
  single file to the manifest.
* The ``PythonExecutable`` Starlark type now exposes a ``to_file_manifest()`` to
  convert the instance to a ``FileManifest``.
* The ``PythonExecutable`` Starlark type now exposes a ``to_wix_msi_builder()``
  method to obtain a ``WiXMSIBuilder``, which can be used to generate an MSI
  installer for the application.
* The ``PythonExecutable`` Starlark type now exposes a ``to_wix_bundle_builder()``
  method to obtain a ``WiXBundleBuilder``, which can be used to generate an
  ``.exe`` installer for the application.
* The ``pyembed`` crate and ``OxidizedFinder`` importer now support indexing
  multiple resources sources. You can have multiple in-memory data blobs,
  multiple file-based resources, or a mix of all of the above.
* The ``OxidizedFinder`` Python type now exposed various ``index_*`` methods
  to facilitate loading/indexing of resource data in arbitrary byte buffers
  or files. You can call these methods multiple times to chain multiple
  resources blobs together.
* The ``PythonExecutable`` Starlark type now exposes a
  ``packed_resources_load_mode`` attribute allowing control over where *packed
  resources data* is written and how it is loaded at run-time. This attribute
  facilitates disabling the embedding of packed resources data completely
  (enabling you to produce an executable that behaves very similarly to
  ``python``) and allows writing and loading resources data to a standalone
  file installed next to the binary (enabling multiple binaries to share the
  same resources file). See :ref:`packaging_resources_data` for more on this
  feature.
* PyOxidizer now scans for licenses of Python packages processed during
  building and prints a report about what it finds when writing build
  artifacts. This feature is best effort and relies on packages properly
  advertising their license metadata.
* Support for configuring Python's memory allocators has been expanded.
  The Starlark :py:attr:`PythonInterpreterConfig.allocator_debug`
  field has been added and allows enabling Python memory allocator debug hooks.
  The Starlark :py:attr:`PythonInterpreterConfig.allocator_mem`,
  :py:attr:`PythonInterpreterConfig.allocator_obj`,
  and :py:attr:`PythonInterpreterConfig.allocator_pymalloc_arena`
  fields have been added to control whether to install a custom allocator for
  the *mem* and *obj* domains as well as ``pymalloc``'s arena allocator.
* The *mimalloc* and *snmalloc* memory allocators can now be used as Python's
  memory allocators. See documentation for
  :py:attr:`PythonInterpreterConfig.allocator_backend`.
  Code contributed by Ryan Clanton in #358.
* The *mimalloc* and *snmalloc* memory allocators will now automatically be used
  as Rust's global allocator when configured to be used by Python.
* The ``@classmethod`` ``OxidizedDistribution.find_name()`` and
  ``OxidizedDistribution.discover()`` are now implemented, filling in a feature
  gap in ``importlib.metadata`` functionality.
* There is a new :py:attr:`PythonExecutable.windows_runtime_dlls_mode`
  attribute to control how required Windows runtime DLL files should be
  materialized during application building. By default, if a built binary
  requires the Visual C++ Redistributable Runtime (e.g. ``vcruntime140.dll``),
  PyOxidizer will attempt to locate and copy those files next to the built
  binary. See :ref:`pyoxidizer_distributing_windows_vc_redist` for more.
* Documentation around portability of binaries produced with PyOxidizer has been
  reorganized and overhauled. See :ref:`pyoxidizer_distributing_binary_portability`
  for the new documentation.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* Python distributions upgraded to CPython 3.8.8 and 3.9.2 (from 3.8.6 and 3.9.0).
  See https://github.com/indygreg/python-build-standalone/releases/tag/20210103
  and https://github.com/indygreg/python-build-standalone/releases/tag/20210227
  for a full list of changes in these distributions.
* CI has been moved from Azure Pipelines to GitHub Actions.
* Low level code in the ``pyembed`` crate for loading and indexing resources
  has been significantly refactored. This code has historically been a bit
  brittle, as it needs to do *unsafe* things. We think the new code is much
  more robust. But there's a chance that crashes could occur.
* When using the ``no-copyleft`` (formerly ``no-gpl``) extension module filter,
  some system library dependencies are now allowed, enabling various extension
  modules to be present in this mode.
* The ``pyembed`` and ``oxidized-importer`` crates had their SPDX license
  expression changed from ``Python-2.0 AND MPL-2.0`` to
  ``Python-2.0 OR MPL-2.0``. The author misunderstood what ``AND`` did and
  after realizing his mistake, corrected it to ``OR`` so the crates can one
  license or the other.
* When using dynamically linked Python distributions on Windows, the
  ``python3.dll`` file is automatically installed if it is present. (#336)
* ``libclang_rt.osx.a`` is now linked into Python binaries on macOS. This
  was necessary to avoid undefined symbols errors from symbols which Python
  3.9.1+ relies on.
* The ``oxidized_importer`` Python module now exports the
  ``OxidizedDistribution`` symbol, which is the custom ``importlib.metadata``
  *distribution* type used by ``OxidizedFinder``.
* When building with Windows ``standalone_static`` distributions, ``pyoxidizer``
  now sets ``RUSTFLAGS=-C target-feature=+crt-static -C link-args=/FORCE:MULTIPLE``
  to force static CRT linkage and ignore duplicate symbol errors. Previously, the
  Python distribution would be using static CRT linkage and the Rust application
  would use dynamic/DLL CRT linkage. Furthermore, many ``standalone_static``
  distributions have build configurations that lead to duplicate symbols and
  this would lead to a linker error. Suppressing the duplicate symbol error
  is not ideal, but it restores building with ``standalone_static`` until a
  more appropriate workaround can be devised.

.. _version_0_10_3:

0.10.3
------

Released November 10, 2020.

Bug Fixes
^^^^^^^^^

* The ``run_as_main()`` function on embedded Python interpreters now always
  calls ``Py_RunMain()``. This fixes a regression in previous 0.10 releases
  that prevented a REPL from running when no explicit ``run_*`` attribute was
  set on the Python interpreter configuration.

.. _version_0_10_2:

0.10.2
------

Released November 10, 2020.

Bug Fixes
^^^^^^^^^

* Fixes a version mismatch between the ``pyoxidizer`` and ``pyembed`` crates
  that could cause builds to fail.

.. _version_0_10_1:

0.10.1
------

Released November 9, 2020.

.. danger::

   The 0.10.1 release has a serious bug where the version of the ``pyembed``
   crate needed to build binaries may not be correct, preventing the build from
   working. Please use a newer release.

Bug Fixes
^^^^^^^^^

.. _version_0_10_0:

0.10.0
------

Released November 8, 2020.

.. danger::

   The 0.10.0 release has a serious Starlark bug preventing PyOxidizer from
   working correctly in many scenarios. Please use a newer release.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* A lot of unused Rust functions for running Python code have been
  removed from the ``pyembed`` crate. The deleted code has not been used
  since the ``PyConfig`` data structure was adopted for running code during
  interpreter initialization. The deleted code was reimplementing
  functionality in CPython and much of it was of questionable quality.
* The built-in Python distributions have been updated to use version
  ``6`` of the standalone distribution format. PyOxidizer only recognizes
  version ``6`` distributions.
* The ``pyembed::OxidizedPythonInterpreterConfig`` Rust struct now contains
  a ``tcl_library`` field to control the value of the `TCL_LIBRARY` environment
  variable.
* The ``pyembed::OxidizedPythonInterpreterConfig`` Rust struct no longer has
  a ``run_mode`` field.
* The ``PythoninterpreterConfig`` Starlark type no longer has a ``run_mode``
  attribute. To define what code to run at interpreter startup, populate a
  ``run_*`` attribute or leave all ``None`` with ``.parse_argv = True`` (the
  default for ``profile = "python"``) to start a REPL.
* Minimum Rust version changed from 1.40 to 1.41 to facilitate using a new
  crate which requires 1.41.
* The default Cargo features of the ``pyembed`` crate now use the default
  Python interpreter detection and linking configuration as determined by the
  ``cpython`` crate. This enables the ``cargo build`` or ``cargo test`` to
  *just work* without having to explicitly specify features.
* The ``python-distributions-extract`` command now receives the path to an
  existing distribution archive via the ``--archive-path`` argument instead
  of an unnamed argument.

Bug Fixes
^^^^^^^^^

* Fixed a broken documentation example for ``glob()``. (#300)
* Fixed a bug where generated Rust code for `Option<PathBuf>` interpreter
  configuration fields was not being generated correctly.
* Fixed serialization of string config options to Rust code that was preventing
  the following attributes of the ``PythonInterpreterConfig`` Starlark type
  from working: ``filesystem_encoding``, ``filesystem_errors``, ``python_path_env``,
  ``run_command``, ``run_module``, ``stdio_encoding``, ``stdio_errors``,
  ``warn_options``, and ``x_options``. (#309)

New Features
^^^^^^^^^^^^

* The ``PythonExecutable`` Starlark type now exposes a
  ``windows_subsystem`` attribute to control the value of Rust's
  ``#![windows_subsystem = "..."]`` attribute. Setting this to ``windows``
  prevents Windows executables from opening a console window when run. (#216)
* The ``PythonExecutable`` Starlark type now exposes a ``tcl_files_path``
  attribute to define a directory to install tcl/tk support files into.
  Setting this attribute enables the use of the ``tkinter`` Python module
  with compatible Python distributions. (#25)
* The ``python-distribution-extract`` CLI command now accepts a
  ``--download-default`` flag to download the default distribution for the
  current platform.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* The Starlark types with special *build* or *run* behavior are now
  explicitly documented.
* The list of glibc and GCC versions used by popular Linux distributions
  has been updated.
* The built-in Linux and macOS Python distributions are now compiled with
  LLVM/Clang 11 (as opposed to 10).
* The built-in Python distributions now use pip 20.2.4 and setuptools 50.3.2.
* The Starlark primitives for defining build system targets have been extracted
  into a new ``starlark-dialect-build-targets`` crate.
* The code for resolving how to reference PyOxidizer's Git repository has
  been rewritten. The resolution is now performed at build time in the
  pyoxidizer crate's ``build.rs``. There now exist environment variables that
  can be specified at crate build time that influence how PyOxidizer constructs
  these references.

.. _version_0_9_0:

0.9.0
-----

Released October 18, 2020.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The ``pyembed::OxidizedPythonInterpreterConfig`` Rust struct now contains
  an ``argv`` field that can be used to control the population of
  ``sys.argv``.
* The ``pyembed::OxidizedPythonInterpreterConfig`` Rust struct now contains
  a ``set_missing_path_configuration`` field that can be used to
  control the automatic run-time population of missing *path configuration*
  fields.
* The ``configure_locale`` interpreter configuration setting is enabled
  by default. (#294)
* The ``pyembed::OxidizedPythonInterpreterConfig`` Rust struct now contains
  an ``exe`` field holding the path of the currently running executable.
* At run-time, the ``program_name`` and ``home`` fields of the embedded
  Python interpreter's path configuration are now always set to the
  currently running executable and its directory, respectively, unless
  explicit values have been provided.
* The packed resource data version has changed from 2 to 3 in order to
  support storing arbitrary file data. Support for reading and writing
  version 2 has been removed. Packed resources blobs will need to be
  regenerated in order to be compatible with new versions of PyOxidizer.
* The ``pyembed::OxidizedPythonInterpreterConfig`` Rust struct had its
  ``packed_resources`` field changed from ``Option<&'a [u8]>`` to
  ``Vec<&'a [u8]>`` so multiple resource inputs can be specified.
* The ``PythonDistribution`` Starlark type no longer has
  ``extension_modules()``, ``package_resources()`` and ``source_modules()``
  methods. Use ``PythonDistribution.python_resources()`` instead.

New Features
^^^^^^^^^^^^

* A ``print(*args)`` function is now exposed to Starlark. This function is
  documented as a Starlark built-in but isn't provided by the Rust Starlark
  implementation by default. So we've implemented it ourselves. (#292)
* The new ``pyoxidizer find-resources`` command can be used to invoke
  PyOxidizer's code for scanning files for resources. This command can be
  used to debug and triage bugs related to PyOxidizer's custom code for
  finding and handling resources.
* Executables built on Windows now embed an application manifest that enables
  long paths support. (#197)
* The Starlark ``PythonPackagingPolicy`` type now exposes an ``allow_files``
  attribute controlling whether files can be added as resources.
* The Starlark ``PythonPackagingPolicy`` type now exposes
  ``file_scanner_classify_files`` and ``file_scanner_emit_files`` attributes
  controlling whether file scanning attempts to classify files and whether
  generic file instances are emitted, respectively.
* The Starlark ``PythonPackagingPolicy`` type now exposes
  ``include_classified_resources`` and ``include_file_resources`` attributes
  to control whether certain classes of resources have their ``add_include``
  attribute set by default.
* The Starlark ``PythonPackagingPolicy`` type now has a
  ``set_resources_handling_mode()`` method to quickly apply a mode for
  resource handling.
* The Starlark ``PythonDistribution`` type now has a ``python_resources()``
  method for obtaining all Python resources associated with the distribution.
* Starlark ``File`` instances can now be added to resource collections via
  ``PythonExecutable.add_python_resource()`` and
  ``PythonExecutable.add_python_resources()``.

Bug Fixes
^^^^^^^^^

* Fix some documentation references to outdated Starlark configuration
  syntax (#291).
* Emit only the ``PythonExtensionModule`` built with our patched distutils
  instead of emitting 2 ``PythonExtensionModule`` for the same named module.
  This should result in compiled Python extension modules being usable as
  built-in extensions instead of being recognized as only shared libraries.
* Fix typo preventing the Starlark method ``PythonExecutable.read_virtualenv()``
  from being defined. (#297)
* The default value of the Starlark ``PythonInterpreterConfig.configure_locale``
  field is ``True`` instead of ``None`` (effectively ``False`` since the
  default ``.profile`` value is ``isolated``). This results in Python's
  encodings being more reasonable by default, which helps ensure
  non-ASCII arguments are interpreted properly. (#294)
* Properly serialize ``module_search_paths`` to Rust code. Before, attempting
  to set ``PythonInterpreterConfig.module_search_paths`` in Starlark would
  result in malformed Rust code being generated. (#298)

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* The ``pyembed`` Rust crate now calls ``PyConfig_SetBytesArgv`` or
  ``PyConfig_SetArgv()`` to initialize argv instead of
  ``PySys_SetObject()``. The encoding of string values should also
  behave more similarly to what ``python`` does.
* The ``pyembed`` tests exercising Python interpreters now run in
  separate processes. Before, Rust would instantiate multiple interpreters
  in the same process. However, CPython uses global variables and APIs
  (like ``setlocale()``) that also make use of globals and process
  reuse resulted in tests not having pristine execution environments.
  All tests now run in isolated processes and should be much more
  resilient.
* When PyOxidizer invokes a subprocess and logs its output, stderr
  is now redirected to stdout and logged as a unified stream. Previously,
  stdout was logged and stderr went to the parent process stderr.
* There now exists :ref:`documentation <packaging_python_executable>`
  on how to create an executable that behaves like ``python``.
* The documentation on binary portability has been overhauled to go in
  much greater detail.
* The list of standard library test packages is now obtained from the
  Python distribution metadata instead of a hardcoded list in PyOxidizer's
  source code.

.. _version_0_8_0:

0.8.0
-----

Released October 12, 2020.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The default Python distributions have been upgraded to CPython
  3.8.6 (from 3.7.7) and support for Python 3.7 has been removed.
* On Windows, the ``default_python_distribution()`` Starlark function
  now defaults to returning a ``standalone_dynamic`` distribution
  variant, meaning that it picks a distribution that can load standalone
  ``.pyd`` Python extension modules by default.
* The *standalone* Python distributions are now validated to be at
  least version 5 of the distribution format. If you are using the
  default Python distributions, this change should not affect you.
* Support for packaging the official Windows embeddable Python
  distributions has been removed. This support was experimental.
  The official Windows embeddable distributions are missing critical
  support files that make them difficult to integrate with PyOxidizer.
* The ``pyembed`` crate now defines a new ``OxidizedPythonInterpreterConfig``
  type to configure Python interpreters. The legacy ``PythonConfig`` type
  has been removed.
* Various ``run_*`` functions on ``pyembed::MainPythonInterpreter`` have
  been moved to standalone functions in the ``pyembed`` crate. The
  ``run_as_main()`` function (which is called by the default Rust
  program that is generated) will always call ``Py_RunMain()`` and
  finalize the interpreter. See the extensive crate docs for move.
* Python resources data in the ``pyembed`` crate is no longer
  annotated with the ``'static`` lifetime. Instances of ``PythonConfig``
  and ``OxidizedPythonInterpreterConfig`` must now be annotated with
  a lifetime for the resources data they hold such that Rust lifetimes
  can be enforced.
* The type of the custom Python importer has been renamed from
  ``PyOxidizerFinder`` to ``OxidizedFinder``.
* The name of the module providing our custom importer has been renamed
  from ``_pyoxidizer_importer`` to ``oxidized_importer``.
* Minimum Rust version changed from 1.36 to 1.40 to allow for upgrading
  various dependencies to modern versions.
* Windows static extension building is possibly broken due to changes to
  ``distutils``. However, since we changed the default configuration to
  not use this build mode, we've deemed this potential regression acceptable
  for the 0.8 release. If it exists, it will hopefully be fixed in the 0.9
  release.
* The ``pip_install()``, ``read_package_root()``, ``read_virtualenv()`` and
  ``setup_py_install()`` methods of the ``PythonDistribution`` Starlark type
  have been moved to the ``PythonExecutable`` type. Existing Starlark config
  files will need to change references accordingly (often by replacing ``dist.``
  with ``exe.``).
* The ``PythonDistribution.extension_modules()`` Starlark function no
  longer accepts arguments ``filter`` and ``preferred_variants``. The
  function now returns every extension in the distribution. The reasons
  for this change were to make code simpler and the justification for
  removing it was rather weak. Please file an issue if this feature loss
  affects you.
* The ``PythonInterpreterConfig`` Starlark type now internally has most of
  its fields defined to ``None`` by default instead of their default values.
* The following Starlark methods have been renamed:
  ``PythonExecutable.add_module_source()`` ->
  ``PythonExecutable.add_python_module_source()``;
  ``PythonExecutable.add_module_bytecode()`` ->
  ``PythonExecutable.add_python_module_bytecode()``;
  ``PythonExecutable.add_package_resource()`` ->
  ``PythonExecutable.add_python_package_resource()``;
  ``PythonExecutable.add_package_distribution_resource()`` ->
  ``PythonExecutable.add_python_package_distribution_resource()``;
  ``PythonExecutable.add_extension_module()`` ->
  ``PythonExecutable.add_python_extension_module()``.
* The location-specific Starlark methods for adding Python resources
  have been removed. The functionality can be duplicated by modifying
  the ``add_location`` and ``add_location_fallback`` attributes on
  Python resource types. The following methods were removed:
  ``PythonExecutable.add_in_memory_module_source()``;
  ``PythonExecutable.add_filesystem_relative_module_source()``,
  ``PythonExecutable.add_in_memory_module_bytecode()``;
  ``PythonExecutable.add_filesystem_relative_module_bytecode()``;
  ``PythonExecutable.add_in_memory_package_resource()``;
  ``PythonExecutable.add_filesystem_relative_package_resource()``;
  ``PythonExecutable.add_in_memory_package_distribution_resource()``
  ``PythonExecutable.add_filesystem_relative_package_distribution_resource()``;
  ``PythonExecutable.add_in_memory_extension_module()``;
  ``PythonExecutable.add_filesystem_relative_extension_module()``;
  ``PythonExecutable.add_in_memory_python_resource()``;
  ``PythonExecutable.add_filesystem_relative_python_resource()``;
  ``PythonExecutable.add_in_memory_python_resources()``;
  ``PythonExecutable.add_filesystem_relative_python_resources()``.
* The Starlark ``PythonDistribution.to_python_executable()`` method
  no longer accepts the arguments ``extension_module_filter``,
  ``preferred_extension_module_variants``, ``include_sources``,
  ``include_resources``, and ``include_test``. All of this functionality
  has been replaced by the optional ``packaging_policy``, which accepts
  a ``PythonPackagingPolicy`` instance. The new type represents all
  settings influencing executable building and control over resources
  added to the executable.
* The Starlark type ``PythonBytecodeModule`` has been removed. Previously,
  this type was internally a request to convert Python module source into
  bytecode. The introduction of ``PythonPackagingPolicy`` and underlying
  abilities to derive bytecode from a Python source module instance when
  adding that resource type rendered this Starlark type redundant. There
  may still be the need for a Starlark type to represent actual Python
  module bytecode (not derived from source code at build/packaging time).
  However, this functionality did not exist before so the loss of this
  type is not a loss in functionality.
* The Starlark methods ``PythonExecutable.add_python_resource()`` and
  ``PythonExecutable.add_python_resources()`` no longer accept the
  arguments ``add_source_module``, ``add_bytecode_module``, and
  ``optimize_level``. Instead, set various ``add_*`` attributes on
  resource instances being passed into the methods to influence what
  happens when they are added.
* The Starlark methods ``PythonExecutable.add_python_module_source()``,
  ``PythonExecutable.add_python_module_bytecode()``,
  ``PythonExecutable.add_python_package_resource()``,
  ``PythonExecutable.add_python_package_distribution_resource()``, and
  ``PythonExecutable.add_python_extension_module()`` have been removed.
  The remaining ``PythonExecutable.add_python_resource()`` and
  ``PythonExecutable.add_python_resources()`` methods are capable of
  handling all resource types and should be used. Previous functionality
  available via argument passing on these methods can be accomplished
  by setting ``add_*`` attributes on individual Python resource objects.
* The Starlark type ``PythonSourceModule`` has been renamed to
  ``PythonModuleSource``.
* Serialized Python resources no longer rely on the ``flavor`` field
  to influence how they are loaded at run-time. Instead, the new
  ``is_*`` fields expressing individual type affinity are used. The
  ``flavor`` attributes from the ``OxidizedResource`` Python type
  has been removed since it does nothing.
* The packed resources data format version has been changed from 1 to 2.
  The parser has dropped support for reading version 1 files. Packed resources
  blobs will need to be written and read by the same version of the Rust
  crate to be compatible.
* The autogenerated Rust file containing the Python interpreter configuration
  now emits a ``pyembed::OxidizedPythonInterpreterConfig`` instance instead
  of ``pyembed::PythonConfig``. The new type is more powerful and what is
  actually used to initialize an embedded Python interpreter.
* The concept of a *resources policy* in Starlark has now largely been
  replaced by attributes denoting valid locations for resources.
* ``oxidized_importer.OxidizedResourceCollector.__init__()`` now
   accepts an ``allowed_locations`` argument instead of ``policy``.
* The ``PythonInterpreterConfig()`` constructor has been removed. Instances
  of this Starlark type are now created via
  ``PythonDistribution.make_python_interpreter_config()``. In addition,
  instances are mutated by setting attributes rather than passing
  perhaps dozens of arguments to a constructor function.
* The default build configuration for Windows no longer forces
  extension modules to be loaded from memory and materializes some
  extension modules as standalone files. This was done because some
  some extension modules weren't working when loaded from memory and the
  configuration caused lots of problems in the wild. The new default should
  be much more user friendly. To use the old settings, construct a custom
  ``PythonPackagingPolicy`` and set
  ``allow_in_memory_shared_library_loading = True`` and
  ``resources_location_fallback = None``.

New Features
^^^^^^^^^^^^

* Python distributions upgraded to CPython 3.8.6.
* CPython 3.9 distributions are now supported by passing
  ``python_version="3.9"`` to the ``default_python_distribution()`` Starlark
  function. CPython 3.8 is the default distribution version.
* Embedded Python interpreters are now managed via the
  `new apis <https://docs.python.org/3/c-api/init_config.htm>`_ defined
  by PEP-587. This gives us much more control over the configuration
  of interpreters.
* A ``FileManifest`` Starlark instance will now have its default
  ``pyoxidizer run`` executable set to the last added Python executable.
  Previously, it would only have a run target if there was a single executable
  file in the ``FileManifest``. If there were multiple executables or
  executable files (such as Python extension modules) a run target would
  not be available and ``pyoxidizer run`` would do nothing.
* Default Python distributions upgraded to version 5 of the
  standalone distribution format. This new format advertises much more
  metadata about the distribution, enabling PyOxidizer to take fewer
  guesses about how the distribution works and will help enable
  more features over time.
* The ``pyembed`` crate now exposes a new ``OxidizedPythonInterpreterConfig``
  type (and associated types) allowing configuration of every field
  supported by Python's interpreter configuration API.
* Resources data loaded by the ``pyembed`` crate can now have a
  non-``'static`` lifetime. This means that resources data can be
  more dynamically obtained (e.g. by reading a file). PyOxidizer does
  not yet support such mechanisms, however.
* ``OxidizedFinder`` instances can now be
  :py:meth:`constructed from Python code <oxidized_importer.OxidizedFinder.__new__>`.
  This means that a Python application can instantiate and install its
  own oxidized module importer.
* The resources indexed by ``OxidizedFinder`` instances are now
  representable to Python code as ``OxidizedResource`` instances. These
  types can be created, queried, and mutated by Python code. See
  :ref:`oxidized_resource` for the API.
* ``OxidizedFinder`` instances can now have custom ``OxidizedResource``
  instances registered against them. This means Python code can collect
  its own Python modules and register them with the importer. See
  :py:meth:`oxidized_importer.OxidizedFinder.add_resource` for more.
* ``OxidizedFinder`` instances can now serialize indexed resources out
  to a ``bytes``. The serialized data can be loaded into a separate
  ``OxidizedFinder`` instance, perhaps in a different process. This
  facility enables the creation and reuse of packed resources data
  structures without having to use ``pyoxidizer`` to collect Python
  resources data.
* The types returned by ``OxidizedFinder.find_distributions()`` now
  implement ``entry_points``, allowing *entry points* to be discovered.
* The types returned by ``OxidizedFinder.find_distributions()`` now
  implement ``requires``, allowing package requirements to be discovered.
* ``OxidizedFinder`` is now able to load Python modules when only source
  code is provided. Previously, it required that bytecode be available.
* ``OxidizedFinder`` now implements ``iter_modules()``. This enables
  ``pkgutil.iter_modules()`` to return modules serviced by ``OxidizedFinder``.
* The ``PythonModuleSource`` Starlark type now exposes module source code
  via the ``source`` attribute.
* The ``PythonExecutable`` Starlark type now has a
  ``make_python_module_source()`` method to allow construction of
  ``PythonModuleSource`` instances.
* The ``PythonModuleSource`` Starlark type now has attributes
  ``add_include``, ``add_location``, ``add_location_fallback``,
  ``add_source``, ``add_bytecode_optimization_level_zero``,
  ``add_bytecode_optimization_level_one``, and
  ``add_bytecode_optimization_level_two`` to influence what happens
  when instances are added to to binaries.
* The Starlark methods for adding Python resources now accept an
  optional ``location`` argument for controlling the load location
  of the resource. This functionality replaces the prior functionality
  provided by location-specific APIs such as
  ``PythonExecutable.add_in_memory_python_resource()``. The following
  methods gained this argument:
  ``PythonExecutable.add_python_module_source()``;
  ``PythonExecutable.add_python_module_bytecode()``;
  ``PythonExecutable.add_python_package_resource()``;
  ``PythonExecutable.add_python_package_distribution_resource()``;
  ``PythonExecutable.add_python_extension_module()``;
  ``PythonExecutable.add_python_resource()``;
  ``PythonExecutable.add_python_resources()``.
* Starlark now has a ``PythonPackagingPolicy`` type to represent the
  collection of settings influencing how Python resources are packaged
  into binaries.
* The ``PythonDistribution`` Starlark type has gained a
  ``make_packaging_policy()`` method for obtaining the default
  ``PythonPackagingPolicy`` for that distribution.
* The ``PythonPackagingPolicy.register_resource_callback()`` method can
  be used to register a Starlark function that will be called whenever
  resources are created. The callback allows a single function to inspect
  and manipulate resources as they are created.
* Starlark types representing Python resources now expose an ``is_stdlib``
  attribute denoting whether they came from the Python distribution.
* The new ``PythonExecutable.pip_download()`` method will run ``pip download``
  to obtain Python wheels for the requested package(s). Those wheels will
  then be parsed for Python resources, which can be added to the executable.
* The Starlark function ``default_python_distribution()`` now accepts a
  ``python_version`` argument to control the *X.Y* version of Python to
  use.
* The ``PythonPackagingPolicy`` Starlark type now exposes a flag to
  control whether shared libraries can be loaded from memory.
* The ``PythonDistribution`` Starlark type now has a
  ``make_python_interpreter_config()`` method to obtain instances of
  ``PythonInterpreterConfig`` that are appropriate for that distribution.
* ``PythonInterpreterConfig`` Starlark types now expose attributes to query
  and mutate state. Nearly every setting exposed by Python's initialization
  API can be set.

Bug Fixes
^^^^^^^^^

* Fixed potential process crash due to illegal memory access when loading
  Python bytecode modules from the filesystem.
* Detection of Python bytecode files based on registered suffixes and
  cache tags is now more robust. Before, it was possible for modules to
  get picked up having the cache tag (e.g. ``cpython-38``) in the module
  name.
* In the custom Python importer, ``read_text()`` of distributions returned
  from ``find_distributions()`` now returns ``None`` on unknown file instead
  of raising ``IOError``. This matches the behavior of ``importlib.metadata``.
* The ``pyembed`` Rust project build script now reruns when the source
  Starlark file changes.
* Some Python resource types were improperly installed in the wrong
  relative directory. The buggy behavior has been fixed.
* Python extension modules and their shared library dependencies loaded from the
  filesystem should no longer have the library file suffix stripped when
  materialized on the filesystem.
* On Windows, the ``sqlite`` module can now be imported. Before, the system
  for serializing resources thought that ``sqlite`` was a shared library
  and not a Python module.
* The build script of the pyoxidizer crate now uses the ``git2`` crate to
  try to resolve the Git commit instead of relying on the ``git`` command.
  This should result in fewer cases where the commit was being identified
  as ``unknown``.
* ``$ORIGIN`` is properly expanded in ``sys.path``. (This was a regression
  during the development of version 0.8 and is not a regression from the
  0.7 release.)

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* The registration of the custom Python importer during interpreter
  initialization no longer relies on running custom frozen bytecode
  for the ``importlib._bootstrap_external`` Python module. This
  simplifies packaging and interpreter configuration a bit.
* Packaging documentation now gives more examples on how to use available
  Starlark packaging methods.
* The modified ``distutils`` files used when building statically linked
  extensions have been upgraded to those based on Python 3.8.3.
* The default ``pyoxidizer.bzl`` now has comments for the ``packaging_policy``
  argument to ``PythonDistribution.to_python_executable()``.
* The default ``pyoxidizer.bzl`` now uses ``add_python_resources()`` instead
  of ``add_in_memory_python_resources()``.
* The Rust Starlark crate was upgraded from version 0.2 to 0.3. There were
  numerous changes as part of this upgrade. While we think behavior should
  be mostly backwards compatible, there may be some slight changes in
  behavior. Please file issues if any odd behavior or regressions are
  observed.
* The configuration documentation was reorganized. The unified document
  for the complete API document (which was the largest single document)
  has been split into multiple documents.
* The serialized data structure for representing Python resources metadata
  and its data now allows resources to identify as multiple types. For
  example, a single resource can contain both Python module source/bytecode
  and a shared library.
* ``pyoxidizer --version`` now prints verbose information about where PyOxidizer
  was installed, what Git commit was used, and how the ``pyembed`` crate will
  be referenced. This should make it easier to help debug installation issues.
* The autogenerated/default Starlark configuration file now uses the ``install``
  target as the default build/run target. This allows extra files required
  by generated binaries to be available and for built binaries to be usable.

.. _version_0_7_0:

0.7.0
-----

Released April 9, 2020.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* Packages imported from memory using PyOxidizer now set ``__path__`` with
  a value formed by joining the current executable's path with the package
  name. This mimics the behavior of ``zipimport``.
* Resolved Python resource names have changed behavior. See the note in the
  bug fixes section below.
* The ``PythonDistribution.to_python_executable()`` Starlark method has added
  a ``packaging_policy`` named argument as its 2nd argument / 1st named
  argument. If you were affected by this, you should add argument names to all
  arguments passed to this method.
* The default Rust project for built executables now builds executables such
  that dynamic symbols are exported from the executable. This change is
  necessary in order to support executables loading Python extension modules,
  which are shared libraries which need access to Python symbols defined
  in executables.
* The ``PythonResourceData`` Starlark type has been renamed to
  ``PythonPackageResource``.
* The ``PythonDistribution.resources_data()`` Starlark method has been
  renamed to ``PythonDistribution.package_resources()``.
* The ``PythonExecutable.to_embedded_data()`` Starlark method has been
  renamed to ``PythonExecutable.to_embedded_resources()``.
* The ``PythonEmbeddedData`` Starlark type has been renamed to
  ``PythonEmbeddedResources``.
* The format of Python resource data embedded in binaries has been completely
  rewritten. The separate modules and resource data structures have been merged
  into a single data structure. Embedded resources data can now express more
  primitives such as package distribution metadata and different bytecode
  optimization levels.
* The `pyembed` crate now has a *dev* dependency on the `pyoxidizer` crate in
  order to run tests.

Bug Fixes
^^^^^^^^^

* PyOxidizer's importer now always sets ``__path__`` on imported packages
  in accordance with Python's stated behavior (#51).
* The mechanism for resolving Python resource files from the filesystem has
  been rewritten. Before, it was possible for files like
  ``package/resources/foo.txt`` to be normalized to a (package, resource_name)
  tuple of `(package, resources.foo.txt)`, which was weird and not compatible
  with Python's resource loading mechanism. Resources in sub-directories should
  no longer encounter munging of directory separators to ``.``. In the above
  example, the resource path will now be expressed as
  ``(package, resources/foo.txt)``.
* Certain packaging actions are only performed once during building instead of
  twice. The user-visible impact of this change is that some duplicate log
  messages no longer appear.
* Added a missing `)` for `add_python_resources()` in auto-generated
  `pyoxidizer.bzl` files.

New Features
^^^^^^^^^^^^

* Python resource scanning now recognizes ``*.dist-info`` and ``*.egg-info``
  directories as package distribution metadata. Files within these directories
  are exposed to Starlark as :py:class:`PythonPackageDistributionResource`
  instances. These resources can be added to the embedded resources payload
  and made available for loading from memory or the filesystem, just like
  any other resource. The custom Python importer implements ``get_distributions()``
  and returns objects that expose package distribution files. However,
  functionality of the returned *distribution* objects is not yet complete.
  See :ref:`packaging_importlib_metadata_compatibility` for details.
* The custom Python importer now implements ``get_data(path)``, allowing loading
  of resources from filesystem paths (#139).
* The ``PythonDistribution.to_python_executable()`` Starlark method now accepts
  a ``packaging_policy`` argument to control a policy and default behavior for
  resources on the produced executable. Using this argument, it is possible
  to control how resources should be materialized. For example, you can specify
  that resources should be loaded from memory if supported and from the filesystem
  if not. The argument can also be used to materialize the Python standard library
  on the filesystem, like how Python distributions typically work.
* Python resources can now be installed next to built binaries using the new
  Starlark functions ``PythonExecutable.add_filesystem_relative_module_source()``,
  ``PythonExecutable.add_filesystem_relative_module_bytecode()``,
  ``PythonExecutable.add_filesystem_relative_package_resource()``,
  ``PythonExecutable.add_filesystem_relative_extension_module()``,
  ``PythonExecutable.add_filesystem_relative_python_resource()``,
  ``PythonExecutable.add_filesystem_relative_package_distribution_resource()``,
  and ``PythonExecutable.add_filesystem_relative_python_resources()``. Unlike
  adding Python resources to ``FileManifest`` instances, Python resources added
  this way have their metadata serialized into the built executable. This allows
  the special Python module importer present in built binaries to service the
  ``import`` request without going through Python's default filesystem-based
  importer. Because metadata for the file-based Python resources is *frozen* into
  the application, Python has to do far less work at run-time to load resources,
  making operations faster. Resources loaded from the filesystem in this manner
  have attributes like ``__file__``, ``__cached__``, and ``__path__`` set,
  emulating behavior of the default Python importer. The custom import now also
  implements the ``importlib.abc.ExecutionLoader`` interface.
* Windows binaries can now import extension modules defined as shared libraries
  (e.g. ``.pyd`` files) from memory. PyOxidizer will detect ``.pyd`` files during
  packaging and embed them into the binary as resources. When the module
  is imported, the extension module/shared library is loaded from memory
  and initialized. This feature enables PyOxidizer to package pre-built
  extension modules (e.g. from Windows binary wheels published on PyPI)
  while still maintaining the property of a (mostly) self-contained
  executable.
* Multiple bytecode optimization levels can now be embedded in binaries.
  Previously, it was only possible to embed bytecode for a given module
  at a single optimization level.
* The ``default_python_distribution()`` Starlark function now accepts values
  ``standalone_static`` and ``standalone_dynamic`` to specify a *standalone*
  distribution that is either statically or dynamically linked.
* Support for parsing version 4 of the ``PYTHON.json`` distribution descriptor
  present in standalone Python distribution archives.
* Default Python distributions upgraded to CPython 3.7.7.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* The directory for downloaded Python distributions in the build directory
  now uses a truncated SHA-256 hash instead of the full hash to help avoid
  path length limit issues (#224).
* The documentation for the ``pyembed`` crate has been moved out of the
  Sphinx documentation and into the Rust crate itself. Rendered docs can be
  seen by following the *Documentation* link at https://crates.io/crates/pyembed
  or by running ``cargo doc`` from a source checkout.

.. _version_0_6_0:

0.6.0
-----

Released February 12, 2020.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* The ``default_python_distribution()`` Starlark function now accepts a ``flavor``
  argument denoting the distribution flavor.
* The ``pyembed`` crate no longer includes the auto-generated default configuration
  file. Instead, it is consumed by the application that instantiates a Python
  interpreter.
* Rust projects for the main executable now utilize and require a Cargo build script
  so metadata can be passed from ``pyembed`` to the project that is consuming it.
* The ``pyembed`` crate is no longer added to created Rust projects. Instead,
  the generated ``Cargo.toml`` will reference a version of the ``pyembed`` crate
  identical to the ``PyOxidizer`` version currently running. Or if ``pyoxidizer``
  is running from a Git checkout of the canonical ``PyOxidizer`` Git repository,
  a local filesystem path will be used.
* The fields of ``EmbeddedPythonConfig`` and ``pyembed::PythonConfig`` have been
  renamed and reordered to align with Python 3.8's config API naming. This was done
  for the Starlark type in version 0.5. We have made similar changes to 0.6 so
  naming is consistent across the various types.

Bug Fixes
^^^^^^^^^

* Module names without a ``.`` are now properly recognized when scanning the
  filesystem for Python resources and a package allow list is used (#223).
  Previously, if filtering scanned resources through an explicit list of allowed
  packages, the top-level module/package without a dot in its full name would not
  be passed through the filter.

New Features
^^^^^^^^^^^^

* The ``PythonDistribution()`` Starlark function now accepts a ``flavor`` argument
  to denote the distribution type. This allows construction of alternate distribution
  types.
* The ``default_python_distribution()`` Starlark function now accepts a
  ``flavor`` argument which can be set to ``windows_embeddable`` to return a
  distribution based on the zip file distributions published by the official
  CPython project.
* The ``pyembed`` crate and generated Rust projects now have various
  ``build-mode-*`` feature flags to control how build artifacts are built. See
  :ref:`rust_projects` for more.
* The ``pyembed`` crate can now be built standalone, without being bound to
  a specific ``PyOxidizer`` configuration.
* The ``register_target()`` Starlark function now accepts an optional
  ``default_build_script`` argument to define the default target when
  evaluating in *Rust build script* mode.
* The ``pyembed`` crate now builds against published ``cpython`` and
  ``python3-sys`` crates instead of a a specific Git commit.
* Embedded Python interpreters can now be configured to run a file specified
  by a filename. See the ``run_file`` argument of
  :py:class:`PythonInterpreterConfig`.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* Rust internals have been overhauled to use traits to represent various types,
  namely Python distributions. The goal is to allow different Python
  distribution flavors to implement different logic for building binaries.
* The ``pyembed`` crate's ``build.rs`` has been tweaked so it can support
  calling out to ``pyoxidizer``. It also no longer has a build dependency
  on ``pyoxidizer``.

.. _version_0_5_1:

0.5.1
-----

Released January 26, 2020.

Bug Fixes
^^^^^^^^^

* Fixed bad Starlark example for building ``black`` in docs.
* Remove resources attached to packages that don't exist. (This was a
  regression in 0.5.)
* Warn on failure to annotate a package. (This was a regression in 0.5.)
* Building embedded Python resources now emits warnings when ``__file__``
  is seen. (This was a regression in 0.5.)
* Missing parent packages are now automatically added when constructing
  embedded resources. (This was a regression in 0.5.)

.. _version_0_5_0:

0.5.0
-----

Released January 26, 2020.

General Notes
^^^^^^^^^^^^^

This release of PyOxidizer is significant rewrite of the previous version.
The impetus for the rewrite is to transition from TOML to Starlark
configuration files. The new configuration file format should allow
vastly greater flexibility for building applications and will unlock a
world of new possibilities.

The transition to Starlark configuration files represented a shift from
static configuration to something more dynamic. This required refactoring
a ton of code.

As part of refactoring code, we took the opportunity to shore up lots
of the code base. PyOxidizer was the project author's first real Rust
project and a lot of bad practices (such as use of `.unwrap()`/panics)
were prevalent. The code mostly now has proper error handling. Another
new addition to the code is unit tests. While coverage still isn't
great, we now have tests performing meaningful packaging activities.
So regressions should hopefully be less common going forward.

Because of the scale of the rewritten code in this release, it is expected
that there are tons of bugs of regressions. This will likely be a transitional
release with a more robust release to follow.

Backwards Compatibility Notes
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* Support for building distributions/installers has been temporarily dropped.
* Support for installing license files has been temporarily dropped.
* Python interpreter configuration setting names have been changed to reflect
  names from Python 3.8's interpreter initialization API.
* ``.egg-info`` directories are now ignored when scanning for Python resources
  on the filesystem (matching the behavior for ``.dist-info`` directories).
* The ``pyoxidizer init`` sub-command has been renamed to ``init-rust-project``.
* The ``pyoxidizer app-path`` sub-command has been removed.
* Support for building distributions has been removed.
* The minimum Rust version to build has been increased from 1.31 to
  1.36. This is mainly due to requirements from the ``starlark``
  crate. We could potentially reduce the minimum version requirements
  again with minimal changes to 3rd party crates.
* PyOxidizer configuration files are now
  `Starlark <https://github.com/bazelbuild/starlark>`_ instead of TOML
  files. The default file name is ``pyoxidizer.bzl`` instead of
  ``pyoxidizer.toml``. All existing configuration files will need to be
  ported to the new format.

Bug Fixes
^^^^^^^^^

* The ``repl`` run mode now properly exits with a non-zero exit code
  if an error occurs.
* Compiled C extensions now properly honor the ``ext_package`` argument
  passed to ``setup()``, resulting in extensions which properly have
  the package name in their extension name (#26).

New Features
^^^^^^^^^^^^

* A `glob()`` function has been added to config files to allow
  referencing existing files on the filesystem.
* The in-memory ``MetaPathFinder`` now implements ``find_module()``.
* A ``pyoxidizer init-config-file`` command has been implemented to create
  just a ``pyoxidizer.bzl`` configuration file.
* A ``pyoxidizer python-distribution-info`` command has been implemented
  to print information about a Python distribution archive.
* The ``EmbeddedPythonConfig()`` config function now accepts a
  ``legacy_windows_stdio`` argument to control the value of
  ``Py_LegacyWindowsStdioFlag`` (#190).
* The ``EmbeddedPythonConfig()`` config function now accepts a
  ``legacy_windows_fs_encoding`` argument to control the value of
  ``Py_LegacyWindowsFSEncodingFlag``.
* The ``EmbeddedPythonConfig()`` config function now accepts an ``isolated``
  argument to control the value of ``Py_IsolatedFlag``.
* The ``EmbeddedPythonConfig()`` config function now accepts a ``use_hash_seed``
  argument to control the value of ``Py_HashRandomizationFlag``.
* The ``EmbeddedPythonConfig()`` config function now accepts an ``inspect``
  argument to control the value of ``Py_InspectFlag``.
* The ``EmbeddedPythonConfig()`` config function now accepts an ``interactive``
  argument to control the value of ``Py_InteractiveFlag``.
* The ``EmbeddedPythonConfig()`` config function now accepts a ``quiet``
  argument to control the value of ``Py_QuietFlag``.
* The ``EmbeddedPythonConfig()`` config function now accepts a ``verbose``
  argument to control the value of ``Py_VerboseFlag``.
* The ``EmbeddedPythonConfig()`` config function now accepts a ``parser_debug``
  argument to control the value of ``Py_DebugFlag``.
* The ``EmbeddedPythonConfig()`` config function now accepts a ``bytes_warning``
  argument to control the value of ``Py_BytesWarningFlag``.
* The ``Stdlib()`` packaging rule now now accepts an optional ``excludes``
  list of modules to ignore. This is useful for removing unnecessary
  Python packages such as ``distutils``, ``pip``, and ``ensurepip``.
* The ``PipRequirementsFile()`` and ``PipInstallSimple()`` packaging rules
  now accept an optional ``extra_env`` dict of extra environment variables
  to set when invoking ``pip install``.
* The ``PipRequirementsFile()`` packaging rule now accepts an optional
  ``extra_args`` list of extra command line arguments to pass to
  ``pip install``.

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* PyOxidizer no longer requires a forked version of the ``rust-cpython``
  project (the ``python3-sys`` and ``cpython`` crates. All changes required
  by PyOxidizer are now present in the official project.

.. _version_0_4_0:

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
  final binary rather than being ignored.

.. _version_0_3_0:

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

.. _version_0_2_0:

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
