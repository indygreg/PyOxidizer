.. _pyembed_interpreter_config:

================================================
Python Interpreter Configuration Data Structures
================================================

This document describes the data structures for configuring the behavior of
a Python interpreter. The data structures are consumed by the ``pyembed`` Rust crate.
All type names should correspond to public symbols in the ``pyembed`` crate.

This documentation is auto-generated from the inline documentation in Rust source
files. Some formatting has been lost as part of the conversion.
See https://docs.rs/pyembed/ for the native Rust API documentation

Structs:

* :ref:`OxidizedPythonInterpreterConfig <pyembed_struct_OxidizedPythonInterpreterConfig>`
* :ref:`PythonInterpreterConfig <pyembed_struct_PythonInterpreterConfig>`

Enums:

* :ref:`MemoryAllocatorBackend <pyembed_enum_MemoryAllocatorBackend>`
* :ref:`PythonInterpreterProfile <pyembed_enum_PythonInterpreterProfile>`
* :ref:`Allocator <pyembed_enum_Allocator>`
* :ref:`BytecodeOptimizationLevel <pyembed_enum_BytecodeOptimizationLevel>`
* :ref:`BytesWarning <pyembed_enum_BytesWarning>`
* :ref:`CheckHashPycsMode <pyembed_enum_CheckHashPycsMode>`
* :ref:`CoerceCLocale <pyembed_enum_CoerceCLocale>`
* :ref:`MultiprocessingStartMethod <pyembed_enum_MultiprocessingStartMethod>`
* :ref:`TerminfoResolution <pyembed_enum_TerminfoResolution>`

.. _pyembed_struct_OxidizedPythonInterpreterConfig:

``OxidizedPythonInterpreterConfig`` Struct
==========================================

Configuration for a Python interpreter.

This type is used to create a ``crate::MainPythonInterpreter``, which manages
a Python interpreter running in the current process.

This type wraps a ``PythonInterpreterConfig``, which is an abstraction over
the low-level C structs (``PyPreConfig`` and ``PyConfig``) used as part of
Python's C initialization API. In addition to this data structure, the
fields on this type facilitate control of additional features provided by
this crate.

The ``PythonInterpreterConfig`` has a single non-optional field:
``PythonInterpreterConfig::profile``. This defines the defaults for various
fields of the ``PyPreConfig`` and ``PyConfig`` C structs. See
https://docs.python.org/3/c-api/init_config.html#isolated-configuration for
more.

When this type is converted to ``PyPreConfig`` and ``PyConfig``, instances
of these C structs are created from the specified profile. e.g. by calling
``PyPreConfig_InitPythonConfig()``, ``PyPreConfig_InitIsolatedConfig``,
``PyConfig_InitPythonConfig``, and ``PyConfig_InitIsolatedConfig``. Then
for each field in ``PyPreConfig`` and ``PyConfig``, if a corresponding field
on ``PythonInterpreterConfig`` is ``Some``, then the ``PyPreConfig`` or
``PyConfig`` field will be updated accordingly.

During interpreter initialization, ``Self::resolve()`` is called to
resolve/finalize any missing values and convert the instance into a
``ResolvedOxidizedPythonInterpreterConfig``. It is this type that is
used to produce a ``PyPreConfig`` and ``PyConfig``, which are used to
initialize the Python interpreter.

Some fields on this type are redundant or conflict with those on
``PythonInterpreterConfig``. Read the documentation of each field to
understand how they interact. Since ``PythonInterpreterConfig`` is defined
in a different crate, its docs are not aware of the existence of
this crate/type.

This struct implements ``Deserialize`` and ``Serialize`` and therefore can be
serialized to any format supported by the ``serde`` crate. This feature is
used by ``pyoxy`` to allow YAML-based configuration of Python interpreters.


.. _pyembed_struct_OxidizedPythonInterpreterConfig_exe:

``exe`` Field
-------------

The path of the currently executing executable.

This value will always be ``Some`` on ``ResolvedOxidizedPythonInterpreterConfig``
instances.

Default value: ``None``.

``Self::resolve()`` behavior: sets to ``std::env::current_exe()`` if not set.
Will canonicalize the final path, which may entail filesystem I/O.

Type: ``Option<PathBuf>``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_origin:

``origin`` Field
----------------

The filesystem path from which relative paths will be interpreted.

This value will always be ``Some`` on ``ResolvedOxidizedPythonInterpreterConfig``
instances.

Default value: ``None``.

``Self::resolve()`` behavior: sets to ``Self::exe.parent()`` if not set.

Type: ``Option<PathBuf>``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_interpreter_config:

``interpreter_config`` Field
----------------------------

Low-level configuration of Python interpreter.

Default value: ``PythonInterpreterConfig::default()`` with
``PythonInterpreterConfig::profile`` always set to ``PythonInterpreterProfile::Python``.

``Self::resolve()`` behavior: most fields are copied verbatim.
``PythonInterpreterConfig::module_search_paths`` entries have the special token
``$ORIGIN`` expanded to the resolved value of ``Self::origin``.

Type: ``PythonInterpreterConfig``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_allocator_backend:

``allocator_backend`` Field
---------------------------

Memory allocator backend to use.

Default value: ``MemoryAllocatorBackend::Default``.

Interpreter initialization behavior: after ``Py_PreInitialize()`` is called,
``crate::pyalloc::PythonMemoryAllocator::from_backend()`` is called. If this
resolves to a ``crate::pyalloc::PythonMemoryAllocator``, that allocator will
be installed as per ``Self::allocator_raw``, ``Self::allocator_mem``,
``Self::allocator_obj``, and ``Self::allocator_pymalloc_arena``. If a custom
allocator backend is defined but all the ``allocator_*`` flags are ``false``,
the allocator won't be used.

Type: ``MemoryAllocatorBackend``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_allocator_raw:

``allocator_raw`` Field
-----------------------

Whether to install the custom allocator for the ``raw`` memory domain.

See https://docs.python.org/3/c-api/memory.html for documentation on how Python
memory allocator domains work.

Default value: ``true``

Interpreter initialization behavior: controls whether ``Self::allocator_backend``
is used for the ``raw`` memory domain.

Has no effect if ``Self::allocator_backend`` is ``MemoryAllocatorBackend::Default``.

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_allocator_mem:

``allocator_mem`` Field
-----------------------

Whether to install the custom allocator for the ``mem`` memory domain.

See https://docs.python.org/3/c-api/memory.html for documentation on how Python
memory allocator domains work.

Default value: ``false``

Interpreter initialization behavior: controls whether ``Self::allocator_backend``
is used for the ``mem`` memory domain.

Has no effect if ``Self::allocator_backend`` is ``MemoryAllocatorBackend::Default``.

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_allocator_obj:

``allocator_obj`` Field
-----------------------

Whether to install the custom allocator for the ``obj`` memory domain.

See https://docs.python.org/3/c-api/memory.html for documentation on how Python
memory allocator domains work.

Default value: ``false``

Interpreter initialization behavior: controls whether ``Self::allocator_backend``
is used for the ``obj`` memory domain.

Has no effect if ``Self::allocator_backend`` is ``MemoryAllocatorBackend::Default``.

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_allocator_pymalloc_arena:

``allocator_pymalloc_arena`` Field
----------------------------------

Whether to install the custom allocator for the ``pymalloc`` arena allocator.

See https://docs.python.org/3/c-api/memory.html for documentation on how Python
memory allocation works.

Default value: ``false``

Interpreter initialization behavior: controls whether ``Self::allocator_backend``
is used for the ``pymalloc`` arena allocator.

This setting requires the ``pymalloc`` allocator to be used for the ``mem``
or ``obj`` domains (``allocator_mem = false`` and ``allocator_obj = false`` - this is
the default behavior) and for ``Self::allocator_backend`` to not be
``MemoryAllocatorBackend::Default``.

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_allocator_debug:

``allocator_debug`` Field
-------------------------

Whether to set up Python allocator debug hooks to detect memory bugs.

Default value: ``false``

Interpreter initialization behavior: triggers the calling of
``PyMem_SetupDebugHooks()`` after custom allocators are installed.

This setting can be used with or without custom memory allocators
(see other ``allocator_*`` fields).

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_set_missing_path_configuration:

``set_missing_path_configuration`` Field
----------------------------------------

Whether to automatically set missing "path configuration" fields.

If ``true``, various path configuration
(https://docs.python.org/3/c-api/init_config.html#path-configuration) fields
will be set automatically if their corresponding ``.interpreter_config``
fields are ``None``. For example, ``program_name`` will be set to the current
executable and ``home`` will be set to the executable's directory.

If this is ``false``, the default path configuration built into libpython
is used.

Setting this to ``false`` likely enables isolated interpreters to be used
with "external" Python installs. If this is ``true``, the default isolated
configuration expects files like the Python standard library to be installed
relative to the current executable. You will need to either ensure these
files are present, define ``packed_resources``, and/or set
``.interpreter_config.module_search_paths`` to ensure the interpreter can find
the Python standard library, otherwise the interpreter will fail to start.

Without this set or corresponding ``.interpreter_config`` fields set, you
may also get run-time errors like
``Could not find platform independent libraries <prefix>`` or
``Consider setting $PYTHONHOME to <prefix>[:<exec_prefix>]``. If you see
these errors, it means the automatic path config resolutions built into
libpython didn't work because the run-time layout didn't match the
build-time configuration.

Default value: ``true``

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_oxidized_importer:

``oxidized_importer`` Field
---------------------------

Whether to install ``oxidized_importer`` during interpreter initialization.

If ``true``, ``oxidized_importer`` will be imported during interpreter
initialization and an instance of ``oxidized_importer.OxidizedFinder``
will be installed on ``sys.meta_path`` as the first element.

If ``Self::packed_resources`` are defined, they will be loaded into the
``OxidizedFinder``.

If ``Self::filesystem_importer`` is ``true``, its *path hook* will be
registered on ``sys.path_hooks`` so ``PathFinder`` (the standard filesystem
based importer) and ``pkgutil`` can use it.

Default value: ``false``

Interpreter initialization behavior: See above.

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_filesystem_importer:

``filesystem_importer`` Field
-----------------------------

Whether to install the path-based finder.

Controls whether to install the Python standard library ``PathFinder`` meta
path finder (this is the meta path finder that loads Python modules and
resources from the filesystem).

Also controls whether to add ``OxidizedFinder``'s path hook to
``sys.path_hooks``.

Due to lack of control over low-level Python interpreter initialization,
the standard library ``PathFinder`` will be registered on ``sys.meta_path``
and ``sys.path_hooks`` for a brief moment when the interpreter is initialized.
If ``sys.path`` contains valid entries that would be serviced by this finder
and ``oxidized_importer`` isn't able to service imports, it is possible for the
path-based finder to be used to import some Python modules needed to initialize
the Python interpreter. In many cases, this behavior is harmless. In all cases,
the path-based importer is removed after Python interpreter initialization, so
future imports won't be serviced by this path-based importer if it is disabled
by this flag.

Default value: ``true``

Interpreter initialization behavior: If false, path-based finders are removed
from ``sys.meta_path`` and ``sys.path_hooks`` is cleared.

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_packed_resources:

``packed_resources`` Field
--------------------------

References to packed resources data.

The format of the data is defined by the ``python-packed-resources``
crate. The data will be parsed as part of initializing the custom
meta path importer during interpreter initialization when
``oxidized_importer=true``. If ``oxidized_importer=false``, this field
is ignored.

If paths are relative, that will be evaluated relative to the process's
current working directory following the operating system's standard
path expansion behavior.

Default value: ``vec![]``

``Self::resolve()`` behavior: ``PackedResourcesSource::MemoryMappedPath`` members
have the special string ``$ORIGIN`` expanded to the string value that
``Self::origin`` resolves to.

This field is ignored during serialization.

Type: ``Vec<PackedResourcesSource>``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_extra_extension_modules:

``extra_extension_modules`` Field
---------------------------------

Extra extension modules to make available to the interpreter.

The values will effectively be passed to ``PyImport_ExtendInitTab()``.

Default value: ``None``

Interpreter initialization behavior: ``PyImport_Inittab`` will be extended
with entries from this list. This makes the extensions available as
built-in extension modules.

This field is ignored during serialization.

Type: ``Option<Vec<ExtensionModule>>``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_argv:

``argv`` Field
--------------

Command line arguments to initialize ``sys.argv`` with.

Default value: ``None``

``Self::resolve()`` behavior: ``Some`` value is used if set. Otherwise
``PythonInterpreterConfig::argv`` is used if set. Otherwise
``std::env::args_os()`` is called.

Interpreter initialization behavior: the resolved ``Some`` value is used
to populate ``PyConfig.argv``.

Type: ``Option<Vec<OsString>>``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_argvb:

``argvb`` Field
---------------

Whether to set ``sys.argvb`` with bytes versions of process arguments.

On Windows, bytes will be UTF-16. On POSIX, bytes will be raw char*
values passed to ``int main()``.

Enabling this feature will give Python applications access to the raw
``bytes`` values of raw argument data passed into the executable. The single
or double width bytes nature of the data is preserved.

Unlike ``sys.argv`` which may chomp off leading argument depending on the
Python execution mode, ``sys.argvb`` has all the arguments used to initialize
the process. i.e. the first argument is always the executable.

Default value: ``false``

Interpreter initialization behavior: ``sys.argvb`` will be set to a
``list[bytes]``. ``sys.argv`` and ``sys.argvb`` should have the same number
of elements.

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_multiprocessing_auto_dispatch:

``multiprocessing_auto_dispatch`` Field
---------------------------------------

Automatically detect and run in ``multiprocessing`` mode.

If set, ``crate::MainPythonInterpreter::run()`` will detect when the invoked
interpreter looks like it is supposed to be a ``multiprocessing`` worker and
will automatically call into the ``multiprocessing`` module instead of running
the configured code.

Enabling this has the same effect as calling ``multiprocessing.freeze_support()``
in your application code's ``__main__`` and replaces the need to do so.

Default value: ``true``

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_multiprocessing_start_method:

``multiprocessing_start_method`` Field
--------------------------------------

Controls how to call ``multiprocessing.set_start_method()``.

Default value: ``MultiprocessingStartMethod::Auto``

Interpreter initialization behavior: if ``Self::oxidized_importer`` is ``true``,
the ``OxidizedImporter`` will be taught to call ``multiprocessing.set_start_method()``
when ``multiprocessing`` is imported. If ``false``, this value has no effect.

Type: ``MultiprocessingStartMethod``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_sys_frozen:

``sys_frozen`` Field
--------------------

Whether to set sys.frozen=True.

Setting this will enable Python to emulate "frozen" binaries, such as
those used by PyInstaller.

Default value: ``false``

Interpreter initialization behavior: If ``true``, ``sys.frozen = True``.
If ``false``, ``sys.frozen`` is not defined.

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_sys_meipass:

``sys_meipass`` Field
---------------------

Whether to set sys._MEIPASS to the directory of the executable.

Setting this will enable Python to emulate PyInstaller's behavior
of setting this attribute. This could potentially help with self-contained
application compatibility by masquerading as PyInstaller and causing code
to activate *PyInstaller mode*.

Default value: ``false``

Interpreter initialization behavior: If ``true``, ``sys._MEIPASS`` will
be set to a ``str`` holding the value of ``Self::origin``. If ``false``,
``sys._MEIPASS`` will not be defined.

Type: ``bool``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_terminfo_resolution:

``terminfo_resolution`` Field
-----------------------------

How to resolve the ``terminfo`` database.

Default value: ``TerminfoResolution::Dynamic``

Interpreter initialization behavior: the ``TERMINFO_DIRS`` environment
variable may be set for this process depending on what ``TerminfoResolution``
instructs to do.

``terminfo`` is not used on Windows and this setting is ignored on that
platform.

Type: ``TerminfoResolution``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_tcl_library:

``tcl_library`` Field
---------------------

Path to use to define the ``TCL_LIBRARY`` environment variable.

This directory should contain an ``init.tcl`` file. It is commonly
a directory named ``tclX.Y``. e.g. ``tcl8.6``.

Default value: ``None``

``Self::resolve()`` behavior: the token ``$ORIGIN`` is expanded to the
resolved value of ``Self::origin``.

Interpreter initialization behavior: if set, the ``TCL_LIBRARY`` environment
variable will be set for the current process.

Type: ``Option<PathBuf>``

.. _pyembed_struct_OxidizedPythonInterpreterConfig_write_modules_directory_env:

``write_modules_directory_env`` Field
-------------------------------------

Environment variable holding the directory to write a loaded modules file.

If this value is set and the environment it refers to is set,
on interpreter shutdown, we will write a ``modules-<random>`` file to
the directory specified containing a ``\n`` delimited list of modules
loaded in ``sys.modules``.

This setting is useful to record which modules are loaded during the execution
of a Python interpreter.

Default value: ``None``

Type: ``Option<String>``


.. _pyembed_struct_PythonInterpreterConfig:

``PythonInterpreterConfig`` Struct
==================================

Holds configuration of a Python interpreter.

This struct holds fields that are exposed by ``PyPreConfig`` and
``PyConfig`` in the CPython API.

Other than the profile (which is used to initialize instances of
``PyPreConfig`` and ``PyConfig``), all fields are optional. Only fields
with ``Some(T)`` will be updated from the defaults.


.. _pyembed_struct_PythonInterpreterConfig_profile:

``profile`` Field
-----------------

Profile to use to initialize pre-config and config state of interpreter.

Type: ``PythonInterpreterProfile``

.. _pyembed_struct_PythonInterpreterConfig_allocator:

``allocator`` Field
-------------------

Name of the memory allocator.

See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.allocator.

Type: ``Option<Allocator>``

.. _pyembed_struct_PythonInterpreterConfig_configure_locale:

``configure_locale`` Field
--------------------------

Whether to set the LC_CTYPE locale to the user preferred locale.

See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.configure_locale.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_coerce_c_locale:

``coerce_c_locale`` Field
-------------------------

How to coerce the locale settings.

See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale.

Type: ``Option<CoerceCLocale>``

.. _pyembed_struct_PythonInterpreterConfig_coerce_c_locale_warn:

``coerce_c_locale_warn`` Field
------------------------------

Whether to emit a warning if the C locale is coerced.

See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale_warn.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_development_mode:

``development_mode`` Field
--------------------------

Whether to enable Python development mode.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.dev_mode.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_isolated:

``isolated`` Field
------------------

Isolated mode.

See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.isolated.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_legacy_windows_fs_encoding:

``legacy_windows_fs_encoding`` Field
------------------------------------

Whether to use legacy filesystem encodings on Windows.

See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.legacy_windows_fs_encoding.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_parse_argv:

``parse_argv`` Field
--------------------

Whether argv should be parsed the way ``python`` parses them.

See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.parse_argv.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_use_environment:

``use_environment`` Field
-------------------------

Whether environment variables are read to control the interpreter configuration.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.use_environment.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_utf8_mode:

``utf8_mode`` Field
-------------------

Controls Python UTF-8 mode.

See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.utf8_mode.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_argv:

``argv`` Field
--------------

Command line arguments.

These will become ``sys.argv``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.argv.

Type: ``Option<Vec<OsString>>``

.. _pyembed_struct_PythonInterpreterConfig_base_exec_prefix:

``base_exec_prefix`` Field
--------------------------

Controls ``sys.base_exec_prefix``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_exec_prefix.

Type: ``Option<PathBuf>``

.. _pyembed_struct_PythonInterpreterConfig_base_executable:

``base_executable`` Field
-------------------------

Controls ``sys._base_executable``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_executable.

Type: ``Option<PathBuf>``

.. _pyembed_struct_PythonInterpreterConfig_base_prefix:

``base_prefix`` Field
---------------------

Controls ``sys.base_prefix``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_prefix.

Type: ``Option<PathBuf>``

.. _pyembed_struct_PythonInterpreterConfig_buffered_stdio:

``buffered_stdio`` Field
------------------------

Controls buffering on ``stdout`` and ``stderr``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.buffered_stdio.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_bytes_warning:

``bytes_warning`` Field
-----------------------

Controls warnings/errors for some bytes type coercions.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.bytes_warning.

Type: ``Option<BytesWarning>``

.. _pyembed_struct_PythonInterpreterConfig_check_hash_pycs_mode:

``check_hash_pycs_mode`` Field
------------------------------

Validation mode for ``.pyc`` files.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.check_hash_pycs_mode.

Type: ``Option<CheckHashPycsMode>``

.. _pyembed_struct_PythonInterpreterConfig_configure_c_stdio:

``configure_c_stdio`` Field
---------------------------

Controls binary mode and buffering on C standard streams.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.configure_c_stdio.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_dump_refs:

``dump_refs`` Field
-------------------

Dump Python references.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.dump_refs.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_exec_prefix:

``exec_prefix`` Field
---------------------

Controls ``sys.exec_prefix``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.exec_prefix.

Type: ``Option<PathBuf>``

.. _pyembed_struct_PythonInterpreterConfig_executable:

``executable`` Field
--------------------

Controls ``sys.executable``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.executable.

Type: ``Option<PathBuf>``

.. _pyembed_struct_PythonInterpreterConfig_fault_handler:

``fault_handler`` Field
-----------------------

Enable ``faulthandler``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.faulthandler.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_filesystem_encoding:

``filesystem_encoding`` Field
-----------------------------

Controls the encoding to use for filesystems/paths.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.filesystem_encoding.

Type: ``Option<String>``

.. _pyembed_struct_PythonInterpreterConfig_filesystem_errors:

``filesystem_errors`` Field
---------------------------

Filesystem encoding error handler.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.filesystem_errors.

Type: ``Option<String>``

.. _pyembed_struct_PythonInterpreterConfig_hash_seed:

``hash_seed`` Field
-------------------

Randomized hash function seed.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.hash_seed.

Type: ``Option<c_ulong>``

.. _pyembed_struct_PythonInterpreterConfig_home:

``home`` Field
--------------

Python home directory.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.home.

Type: ``Option<PathBuf>``

.. _pyembed_struct_PythonInterpreterConfig_import_time:

``import_time`` Field
---------------------

Whether to profile ``import`` time.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.import_time.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_inspect:

``inspect`` Field
-----------------

Enter interactive mode after executing a script or a command.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.inspect.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_install_signal_handlers:

``install_signal_handlers`` Field
---------------------------------

Whether to install Python signal handlers.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.install_signal_handlers.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_interactive:

``interactive`` Field
---------------------

Whether to enable the interactive REPL mode.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.interactive.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_legacy_windows_stdio:

``legacy_windows_stdio`` Field
------------------------------

Controls legacy stdio behavior on Windows.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.legacy_windows_stdio.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_malloc_stats:

``malloc_stats`` Field
----------------------

Whether to dump statistics from the ``pymalloc`` allocator on exit.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.malloc_stats.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_module_search_paths:

``module_search_paths`` Field
-----------------------------

Defines ``sys.path``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.module_search_paths.

This value effectively controls the initial value of ``sys.path``.

The special string ``$ORIGIN`` in values will be expanded to the absolute path of the
directory of the executable at run-time. For example, if the executable is
``/opt/my-application/pyapp``, ``$ORIGIN`` will expand to ``/opt/my-application`` and the
value ``$ORIGIN/lib`` will expand to ``/opt/my-application/lib``.

Type: ``Option<Vec<PathBuf>>``

.. _pyembed_struct_PythonInterpreterConfig_optimization_level:

``optimization_level`` Field
----------------------------

Bytecode optimization level.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.optimization_level.

This setting is only relevant if ``write_bytecode`` is true and Python modules are
being imported from the filesystem using Python’s standard filesystem importer.

Type: ``Option<BytecodeOptimizationLevel>``

.. _pyembed_struct_PythonInterpreterConfig_parser_debug:

``parser_debug`` Field
----------------------

Parser debug mode.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.parser_debug.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_pathconfig_warnings:

``pathconfig_warnings`` Field
-----------------------------

Whether calculating the Python path configuration can emit warnings.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pathconfig_warnings.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_prefix:

``prefix`` Field
----------------

Defines ``sys.prefix``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.prefix.

Type: ``Option<PathBuf>``

.. _pyembed_struct_PythonInterpreterConfig_program_name:

``program_name`` Field
----------------------

Program named used to initialize state during path configuration.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.program_name.

Type: ``Option<PathBuf>``

.. _pyembed_struct_PythonInterpreterConfig_pycache_prefix:

``pycache_prefix`` Field
------------------------

Directory where ``.pyc`` files are written.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pycache_prefix.

Type: ``Option<PathBuf>``

.. _pyembed_struct_PythonInterpreterConfig_python_path_env:

``python_path_env`` Field
-------------------------

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pythonpath_env.

Type: ``Option<String>``

.. _pyembed_struct_PythonInterpreterConfig_quiet:

``quiet`` Field
---------------

Quiet mode.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.quiet.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_run_command:

``run_command`` Field
---------------------

Value of the ``-c`` command line option.

Effectively defines Python code to evaluate in ``Py_RunMain()``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_command.

Type: ``Option<String>``

.. _pyembed_struct_PythonInterpreterConfig_run_filename:

``run_filename`` Field
----------------------

Filename passed on the command line.

Effectively defines the Python file to run in ``Py_RunMain()``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_filename.

Type: ``Option<PathBuf>``

.. _pyembed_struct_PythonInterpreterConfig_run_module:

``run_module`` Field
--------------------

Value of the ``-m`` command line option.

Effectively defines the Python module to run as ``__main__`` in ``Py_RunMain()``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_module.

Type: ``Option<String>``

.. _pyembed_struct_PythonInterpreterConfig_show_ref_count:

``show_ref_count`` Field
------------------------

Whether to show the total reference count at exit.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.show_ref_count.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_site_import:

``site_import`` Field
---------------------

Whether to import the ``site`` module at startup.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.site_import.

The ``site`` module is typically not needed for standalone applications and disabling
it can reduce application startup time.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_skip_first_source_line:

``skip_first_source_line`` Field
--------------------------------

Whether to skip the first line of ``Self::run_filename``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.skip_source_first_line.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_stdio_encoding:

``stdio_encoding`` Field
------------------------

Encoding of ``sys.stdout``, ``sys.stderr``, and ``sys.stdin``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.stdio_encoding.

Type: ``Option<String>``

.. _pyembed_struct_PythonInterpreterConfig_stdio_errors:

``stdio_errors`` Field
----------------------

Encoding error handler for ``sys.stdout`` and ``sys.stdin``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.stdio_errors.

Type: ``Option<String>``

.. _pyembed_struct_PythonInterpreterConfig_tracemalloc:

``tracemalloc`` Field
---------------------

Whether to enable ``tracemalloc``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.tracemalloc.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_user_site_directory:

``user_site_directory`` Field
-----------------------------

Whether to add the user site directory to ``sys.path``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.user_site_directory.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_verbose:

``verbose`` Field
-----------------

Verbose mode.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.verbose.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_warn_options:

``warn_options`` Field
----------------------

Options of the ``warning`` module to control behavior.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.warnoptions.

Type: ``Option<Vec<String>>``

.. _pyembed_struct_PythonInterpreterConfig_write_bytecode:

``write_bytecode`` Field
------------------------

Controls ``sys.dont_write_bytecode``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.write_bytecode.

Type: ``Option<bool>``

.. _pyembed_struct_PythonInterpreterConfig_x_options:

``x_options`` Field
-------------------

Values of the ``-X`` command line options / ``sys._xoptions``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.xoptions.

Type: ``Option<Vec<String>>``


.. _pyembed_enum_MemoryAllocatorBackend:

``MemoryAllocatorBackend`` Enum
===============================

Defines a backend for a memory allocator.

This says which memory allocator API / library to configure the Python
interpreter to use.

Not all allocators are available in all program builds.

Serialization type: ``string``


``Default`` Variant
   The default allocator as configured by Python.
   
   This likely utilizes the system default allocator, normally the
   ``malloc()``, ``free()``, etc functions from the libc implementation being
   linked against.
   
   Serialized value: ``default``
   

``Jemalloc`` Variant
   Use the jemalloc allocator.
   
   Requires the binary to be built with jemalloc support.
   
   Never available on Windows.
   
   Serialized value: ``jemalloc``
   

``Mimalloc`` Variant
   Use the mimalloc allocator (https://github.com/microsoft/mimalloc).
   
   Requires the binary to be built with mimalloc support.
   
   Serialized value: ``mimalloc``
   

``Snmalloc`` Variant
   Use the snmalloc allocator (https://github.com/microsoft/snmalloc).
   
   Not always available.
   
   Serialized value: ``snmalloc``
   

``Rust`` Variant
   Use Rust's global allocator.
   
   The Rust allocator is less efficient than other allocators because of
   overhead tracking allocations. For optimal performance, use the default
   allocator. Or if Rust is using a custom global allocator, use the enum
   variant corresponding to that allocator.
   
   Serialized value: ``rust``
   


.. _pyembed_enum_PythonInterpreterProfile:

``PythonInterpreterProfile`` Enum
=================================

Defines the profile to use to configure a Python interpreter.

This effectively provides a template for seeding the initial values of
``PyPreConfig`` and ``PyConfig`` C structs.

Serialization type: ``string``.


``Isolated`` Variant
   Python is isolated from the system.
   
   See https://docs.python.org/3/c-api/init_config.html#isolated-configuration.
   
   Serialized value: ``isolated``
   

``Python`` Variant
   Python interpreter behaves like ``python``.
   
   See https://docs.python.org/3/c-api/init_config.html#python-configuration.
   
   Serialized value: ``python``
   


.. _pyembed_enum_Allocator:

``Allocator`` Enum
==================

Name of the Python memory allocators.

See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.allocator.

Serialization type: ``string``


``NotSet`` Variant
   Don’t change memory allocators (use defaults).
   
   Serialized value: ``not-set``
   

``Default`` Variant
   Default memory allocators.
   
   Serialized value: ``default``
   

``Debug`` Variant
   Default memory allocators with debug hooks.
   
   Serialized value: ``debug``
   

``Malloc`` Variant
   Use ``malloc()`` from the C library.
   
   Serialized value: ``malloc``
   

``MallocDebug`` Variant
   Force usage of ``malloc()`` with debug hooks.
   
   Serialized value: ``malloc-debug``
   

``PyMalloc`` Variant
   Python ``pymalloc`` allocator.
   
   Serialized value: ``py-malloc``
   

``PyMallocDebug`` Variant
   Python ``pymalloc`` allocator with debug hooks.
   
   Serialized value: ``py-malloc-debug``
   


.. _pyembed_enum_BytecodeOptimizationLevel:

``BytecodeOptimizationLevel`` Enum
==================================

An optimization level for Python bytecode.

Serialization type: ``int``


``Zero`` Variant
   Optimization level 0.
   
   Serialized value: ``0``
   

``One`` Variant
   Optimization level 1.
   
   Serialized value: ``1``
   

``Two`` Variant
   Optimization level 2.
   
   Serialized value: ``2``
   


.. _pyembed_enum_BytesWarning:

``BytesWarning`` Enum
=====================

Defines what to do when comparing ``bytes`` or ``bytesarray`` with ``str`` or comparing ``bytes`` with ``int``.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.bytes_warning.

Serialization type: ``string``


``None`` Variant
   Do nothing.
   
   Serialization value: ``none``
   

``Warn`` Variant
   Issue a warning.
   
   Serialization value: ``warn``
   

``Raise`` Variant
   Raise a ``BytesWarning``.
   
   Serialization value: ``raise``
   


.. _pyembed_enum_CheckHashPycsMode:

``CheckHashPycsMode`` Enum
==========================

Control the validation behavior of hash-based .pyc files.

See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.check_hash_pycs_mode.

Serialization type: ``string``


``Always`` Variant
   Hash the source file for invalidation regardless of value of the ``check_source`` flag.
   
   Serialized value: ``always``
   

``Never`` Variant
   Assume that hash-based pycs always are valid.
   
   Serialized value: ``never``
   

``Default`` Variant
   The ``check_source`` flag in hash-based pycs determines invalidation.
   
   Serialized value: ``default``
   


.. _pyembed_enum_CoerceCLocale:

``CoerceCLocale`` Enum
======================

Holds values for ``coerce_c_locale``.

See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale.

Serialization type: ``string``


``LCCtype`` Variant
   Read the LC_CTYPE locale to decide if it should be coerced.
   
   Serialized value: ``LC_CTYPE``
   

``C`` Variant
   Coerce the C locale.
   
   Serialized value: ``C``
   


.. _pyembed_enum_MultiprocessingStartMethod:

``MultiprocessingStartMethod`` Enum
===================================

Defines how to call ``multiprocessing.set_start_method()`` when ``multiprocessing`` is imported.

When set to a value that is not ``none``, when ``oxidized_importer.OxidizedFinder`` services
an import of the ``multiprocessing`` module, it will automatically call
``multiprocessing.set_start_method()`` to configure how worker processes are created.

If the ``multiprocessing`` module is not imported by ``oxidized_importer.OxidizedFinder``,
this setting has no effect.

Serialization type: ``string``


``None`` Variant
   Do not call ``multiprocessing.set_start_method()``.
   
   This mode is what Python programs do by default.
   
   Serialized value: ``none``
   

``Fork`` Variant
   Call with value ``fork``.
   
   Serialized value: ``fork``
   

``ForkServer`` Variant
   Call with value ``forkserver``
   
   Serialized value: ``forkserver``
   

``Spawn`` Variant
   Call with value ``spawn``
   
   Serialized value: ``spawn``
   

``Auto`` Variant
   Call with a valid appropriate for the given environment.
   
   This likely maps to ``spawn`` on Windows and ``fork`` on non-Windows.
   
   Serialized value: ``auto``
   


.. _pyembed_enum_TerminfoResolution:

``TerminfoResolution`` Enum
===========================

Defines ``terminfo`` database resolution semantics.

Python links against libraries like ``readline``, ``libedit``, and ``ncurses``
which need to utilize a ``terminfo`` database (a set of files defining
terminals and their capabilities) in order to work properly.

The absolute path to the terminfo database is typically compiled into these
libraries at build time. If the compiled path on the building machine doesn't
match the path on the runtime machine, these libraries cannot find the terminfo
database and terminal interactions won't work correctly because these libraries
don't know how to resolve terminal features. This can result in quirks like
the backspace key not working in prompts.

The ``pyembed`` Rust crate is able to point libraries at a terminfo database
at runtime, overriding the compiled-in default path. This enum is used
to control that behavior.

Serialization type: ``string``.


``Dynamic`` Variant
   Resolve ``terminfo`` database using appropriate behavior for current OS.
   
   We will look for the terminfo database in paths that are common for the
   current OS / distribution. The terminfo database is present in most systems
   (except the most barebones containers or sandboxes) and this method is
   usually successfully in locating the terminfo database.
   
   Serialized value: ``dynamic``
   

``None`` Variant
   Do not attempt to resolve the ``terminfo`` database. Basically a no-op.
   
   This is what should be used for applications that don't interact with the
   terminal. Using this option will prevent some I/O syscalls that would
   be incurred by ``dynamic``.
   
   Serialized value: ``none``
   

``Static`` Variant
   Use a specified string as the ``TERMINFO_DIRS`` value.
   
   Serialized value: ``static:<path>``
   
   e.g. ``static:/usr/share/terminfo``.
   

