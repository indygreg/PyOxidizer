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

.. _version_0_10_0:

0.10.0
------

Not yet released.

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

Bug Fixes
^^^^^^^^^

* Fixed a broken documentation example for ``glob()``. (#300)
* Fixed a bug where generated Rust code for `Option<PathBuf>` interpreter
  configuration fields was not being generated correctly.

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

Other Relevant Changes
^^^^^^^^^^^^^^^^^^^^^^

* The Starlark types with special *build* or *run* behavior are now
  explicitly documented.
* The list of glibc and GCC versions used by popular Linux distributions
  has been updated.
* The built-in Linux and macOS Python distributions are now compiled with
  LLVM/Clang 11 (as opposed to 10).
* The built-in Python distributions now use pip 20.2.4 and setuptools 50.3.2.

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
* The ``PythonInterpreterConfig`` Starlark type now interally has most of
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
  :ref:`constructed from Python code <oxidized_finder__new__>`.
  This means that a Python application can instantiate and install its
  own oxidized module importer.
* The resources indexed by ``OxidizedFinder`` instances are now
  representable to Python code as ``OxidizedResource`` instances. These
  types can be created, queried, and mutated by Python code. See
  :ref:`oxidized_resource` for the API.
* ``OxidizedFinder`` instances can now have custom ``OxidizedResource``
  instances registered against them. This means Python code can collect
  its own Python modules and register them with the importer. See
  :ref:`oxidized_finder_add_resource` for more.
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
  are exposed to Starlark as :ref:`config_type_python_package_distribution_resource`
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
  :ref:`config_type_python_interpreter_config`.

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

* A :ref:`config_glob` function has been added to config files to allow
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
