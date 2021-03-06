.. _config_type_python_interpreter_config:

===========================
``PythonInterpreterConfig``
===========================

This type configures the default behavior of the embedded Python interpreter.

Embedded Python interpreters are configured and instantiated using a
Rust ``pyembed::OxidizedPythonInterpreterConfig`` data structure. The
``pyembed`` crate defines a default instance of this data structure with
parameters defined by the settings in this type.

.. note::

   If you are writing custom Rust code and constructing a custom
   ``pyembed::OxidizedPythonInterpreterConfig`` instance and don't use the
   default instance, this config type is not relevant to you and can be
   omitted from your config file.

.. danger::

   Some of the settings exposed by Python's initialization APIs are
   extremely low level and brittle. Various combinations can cause
   the process to crash/exit ungracefully. Be very cautious when setting
   these low-level settings.

Constructors
============

Instances are constructed by calling
:ref:`config_python_distribution_make_python_interpreter_config`.

Attributes
==========

The ``PythonInterpreterConfig`` state is managed via attributes.

There are a ton of attributes and most attributes are not relevant
to most applications. The bulk of the attributes exist to give full
control over Python interpreter initialization.

.. _config_type_python_interpreter_config_pyembed:

Attributes For Controlling ``pyembed`` Features
-----------------------------------------------

This section documents attributes for controlling features
provided by the ``pyembed`` Rust crate, which manages the embedded
Python interpreter at run-time.

These attributes provide features and level of control over
embedded Python interpreters beyond what is possible with Python's
`initialization C API <https://docs.python.org/3/c-api/init_config.html>`_.

.. _config_type_python_interpreter_config_allocator_backend:

``allocator_backend``
^^^^^^^^^^^^^^^^^^^^^

(``string``)

Configures a custom memory allocator to be used by Python.

Accepted values are:

``default``
   Let Python choose how to configure the allocator.

   This will likely use the ``malloc()``, ``free()``, etc functions
   linked to the binary.

``jemalloc``
   Use the jemalloc allocator.

   (Not available on Windows.)

``mimalloc``
   Use the mimalloc allocator (https://github.com/microsoft/mimalloc).

``rust``
   Use Rust's global allocator (whatever that may be).

``snmalloc``
   Use the snmalloc allocator (https://github.com/microsoft/snmalloc).

The ``jemalloc``, ``mimalloc``, and ``snmalloc`` allocators require the
presence of additional Rust crates. A run-time error will occur if these
allocators are configured but the binary was built without these crates.
(This should not occur when using ``pyoxidizer`` to build the binary.)

When a custom allocator is configured, the autogenerated Rust crate
used to build the binary will configure the Rust global allocator
(``#[global_allocator] attribute``) to use the specified allocator.

.. important::

   The ``rust`` allocator is not recommended because it introduces performance
   overhead. But it may help with debugging in some situations.

.. note::

   Both ``mimalloc`` and ``snmalloc`` require the ``cmake`` build tool
   to compile code as part of their build process. If this tool is not
   available in the build environment, you will encounter a build error
   with a message similar to ``failed to execute command: The system
   cannot find the file specified. (os error 2) is `cmake` not installed?``.

   The workaround is to install cmake or use a different allocator.

.. note::

   ``snmalloc`` only supports targeting to macOS 10.14 or newer. You will
   likely see build errors when building a binary targeting macOS 10.13 or
   older.

Default is ``jemalloc`` on non-Windows targets and ``default`` on Windows.
(The ``jemalloc-sys`` crate doesn't work on Windows MSVC targets.)

.. _config_type_python_interpreter_config_allocator_raw:

``allocator_raw``
^^^^^^^^^^^^^^^^^

(``bool``)

Controls whether to install a custom allocator (defined by
``allocator_backend``) into Python's *raw* allocator domain
(``PYMEM_DOMAIN_RAW`` in Python C API speak).

Setting this to ``True`` will replace the system allocator (e.g. ``malloc()``,
``free()``) for this domain.

A value of ``True`` only has an effect if ``allocator_backend`` is some value
other than ``default``.

Defaults to ``True``.

.. _config_type_python_interpreter_config_allocator_mem:

``allocator_mem``
^^^^^^^^^^^^^^^^^

(``bool``)

Controls whether to install a custom allocator (defined by
``allocator_backend``) into Python's *mem* allocator domain
(``PYMEM_DOMAIN_MEM`` in Python C API speak).

Setting this to ``True`` will replace ``pymalloc`` as the allocator
for this domain.

A value of ``True`` only has an effect if ``allocator_backend`` is some value
other than ``default``.

Defaults to ``False``.

.. _config_type_python_interpreter_config_allocator_obj:

``allocator_obj``
^^^^^^^^^^^^^^^^^

(``bool``)

Controls whether to install a custom allocator (defined by
``allocator_backend``) into Python's *obj* allocator domain
(``PYMEM_DOMAIN_OBJ`` in Python C API speak).

Setting this to ``True`` will replace ``pymalloc`` as the allocator
for this domain.

A value of ``True`` only has an effect if ``allocator_backend`` is some value
other than ``default``.

Defaults to ``False``.

.. _config_type_python_interpreter_config_allocator_pymalloc_arena:

``allocator_pymalloc_arena``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^

(``bool``)

Controls whether to install a custom allocator (defined by
``allocator_backend``) into Python's ``pymalloc`` to be used as its
arena allocator.

The ``pymalloc`` allocator is used by Python by default and will use
the system's allocator functions (``malloc()``, ``VirtualAlloc()``, etc)
by default.

Setting this to ``True`` will have no effect if ``pymalloc`` is not
being used (the ``allocator_mem`` and ``allocator_obj`` settings are
``True`` and have replaced ``pymalloc`` as the allocator backend for these
domains).

A value of ``True`` only has an effect if ``allocator_backend`` is some
value other than ``default``.

Defaults to ``False``.

.. _config_type_python_interpreter_config_allocator_debug:

``allocator_debug``
^^^^^^^^^^^^^^^^^^^

(``bool``)

Whether to enable debug hooks for Python's memory allocators.

Enabling debug hooks enables debugging of memory-related issues in the
Python interpreter. This setting effectively controls whether to call
`PyMem_SetupDebugHooks() <https://docs.python.org/3/c-api/memory.html#c.PyMem_SetupDebugHooks>`_
during interpreter initialization. See the linked documentation for more.

Defaults to ``False``.

.. _config_type_python_interpreter_config_oxidized_importer:

``oxidized_importer``
^^^^^^^^^^^^^^^^^^^^^

(``bool``)

Whether to install the ``oxidized_importer`` meta path importer
(:ref:`oxidized_importer`) on ``sys.meta_path`` during interpreter
initialization.

Defaults to ``True``.

.. _config_type_python_interpreter_config_filesystem_importer:

``filesystem_importer``
^^^^^^^^^^^^^^^^^^^^^^^

(``bool``)

Whether to install the standard library path-based importer for
loading Python modules from the filesystem.

If not enabled, Python modules will not be loaded from the filesystem
(via ``sys.path`` discovery): only modules indexed by ``oxidized_importer``
will be loadable.

The filesystem importer is enabled automatically if
:ref:`config_type_python_interpreter_config_module_search_paths` is
non-empty.

.. _config_type_python_interpreter_config_argvb:

``argvb``
^^^^^^^^^

(``bool``)

Whether to expose a ``sys.argvb`` attribute containing ``bytes`` versions
of process arguments.

On platforms where the process receives ``char *`` arguments, Python
normalizes these values to ``unicode`` and makes them available via
``sys.argv``. On platforms where the process receives ``wchar_t *``
arguments, Python may interpret the bytes as a certain encoding.
This encoding normalization can be lossy.

Enabling this feature will give Python applications access to the raw
``bytes`` values of arguments that are actually used. The single or
double width bytes nature of the data is preserved.

Unlike ``sys.argv`` which may chomp off leading arguments depending
on the Python execution mode, ``sys.argvb`` has all the arguments
used to initialize the process. The first argument is always the
executable.

.. _config_type_python_interpreter_config_sys_frozen:

``sys_frozen``
^^^^^^^^^^^^^^

(``bool``)

Controls whether to set the ``sys.frozen`` attribute to ``True``. If
``false``, ``sys.frozen`` is not set.

Default is ``False``.

.. _config_type_python_interpreter_config_sys_meipass:

``sys_meipass``
^^^^^^^^^^^^^^^

(``bool``)

Controls whether to set the ``sys._MEIPASS`` attribute to the path of
the executable.

Setting this and ``sys_frozen`` to ``True`` will emulate the
`behavior of PyInstaller <https://pyinstaller.readthedocs.io/en/v3.3.1/runtime-information.html>`_
and could possibly help self-contained applications that are aware of
PyInstaller also work with PyOxidizer.

Default is ``False``.

.. _config_type_python_interpreter_config_terminfo_resolution:

``terminfo_resolution``
^^^^^^^^^^^^^^^^^^^^^^^

(``string``)

Defines how the terminal information database (``terminfo``) should be
configured.

See :ref:`terminfo_database` for more about terminal databases.

Accepted values are:

``dynamic``
   Looks at the currently running operating system and attempts to do something
   reasonable.

   For example, on Debian based distributions, it will look for the ``terminfo``
   database in ``/etc/terminfo``, ``/lib/terminfo``, and ``/usr/share/terminfo``,
   which is how Debian configures ``ncurses`` to behave normally. Similar
   behavior exists for other recognized operating systems.

   If the operating system is unknown, PyOxidizer falls back to looking for the
   ``terminfo`` database in well-known directories that often contain the
   database (like ``/usr/share/terminfo``).

``none``
   The value ``none`` indicates that no configuration of the ``terminfo``
   database path should be performed. This is useful for applications that
   don't interact with terminals. Using ``none`` can prevent some filesystem
   I/O at application startup.

``static:<path>``
   Indicates that a static path should be used for the path to the ``terminfo``
   database.

   This values consists of a ``:`` delimited list of filesystem paths
   that ``ncurses`` should be configured to use. This value will be used to
   populate the ``TERMINFO_DIRS`` environment variable at application run time.

``terminfo`` is not used on Windows and this setting is ignored on that
platform.

.. _config_type_python_interpreter_config_write_modules_directory_env:

``write_modules_directory_env``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Environment variable that defines a directory where ``modules-<UUID>`` files
containing a ``\n`` delimited list of loaded Python modules (from ``sys.modules``)
will be written upon interpreter shutdown.

If this setting is not defined or if the environment variable specified by its
value is not present at run-time, no special behavior will occur. Otherwise,
the environment variable's value is interpreted as a directory, that directory
and any of its parents will be created, and a ``modules-<UUID>`` file will
be written to the directory.

This setting is useful for determining which Python modules are loaded when
running Python code.

.. _config_type_python_interpreter_config_pypreconfig:

Attributes From ``PyPreConfig``
-------------------------------

Attributes in this section correspond to fields of the
`PyPreConfig <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig>`_
C struct used to initialize the Python interpreter.

.. _config_type_python_interpreter_config_config_profile:

``config_profile``
^^^^^^^^^^^^^^^^^^

(``string``)

This attribute controls which set of default values to use for
attributes that aren't explicitly defined. It effectively controls
which C API to use to initialize the ``PyPreConfig`` instance.

Accepted values are:

``isolated``
   Use the `isolated <https://docs.python.org/3/c-api/init_config.html#isolated-configuration>`_
   configuration.

   This configuration is appropriate for applications existing in isolation
   and not behaving like ``python`` executables.

``python``
   Use the `Python <https://docs.python.org/3/c-api/init_config.html#python-configuration>`_
   configuration.

   This configuration is appropriate for applications attempting to behave
   like a ``python`` executable would.

.. _config_type_python_interpreter_config_allocator:

``allocator``
^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyPreConfig.allocator <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.allocator>`_.

Accepted values are:

``None``
   Use the default.

``not-set``
   ``PYMEM_ALLOCATOR_NOT_SET``

``default``
   ``PYMEM_ALLOCATOR_DEFAULT``

``debug``
   ``PYMEM_ALLOCATOR_DEBUG``

``malloc``
   ``PYMEM_ALLOCATOR_MALLOC``

``malloc-debug``
   ``PYMEM_ALLOCATOR_MALLOC_DEBUG``

``py-malloc``
   ``PYMEM_ALLOCATOR_PYMALLOC``

``py-malloc-debug``
   ``PYMEM_ALLOCATOR_PYMALLOC_DEBUG``

.. _config_type_python_interpreter_config_configure_locale:

``configure_locale``
^^^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyPreConfig.configure_locale <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.configure_locale>`_.

.. _config_type_python_interpreter_config_coerce_c_locale:

``coerce_c_locale``
^^^^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyPreConfig.coerce_c_locale <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale>`_.

Accepted values are:

``LC_CTYPE``
   Read ``LC_CTYPE``

``C``
   Coerce the ``C`` locale.

.. _config_type_python_interpreter_config_coerce_c_locale_warn:

``coerce_c_locale_warn``
^^^^^^^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyPreConfig.coerce_c_locale_warn <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale_warn>`_.

.. _config_type_python_interpreter_config_development_mode:

``development_mode``
^^^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyPreConfig.development_mode <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.development_mode>`_.

.. _config_type_python_interpreter_config_isolated:

``isolated``
^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyPreConfig.isolated <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.isolated>`_.

.. _config_type_python_interpreter_config_legacy_windows_fs_encoding:

``legacy_windows_fs_encoding``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyPreConfig.legacy_windows_fs_encoding <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.legacy_windows_fs_encoding>`_.

.. _config_type_python_interpreter_config_parse_argv:

``parse_argv``
^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyPreConfig.parse_argv <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.parse_argv>`_.

.. _config_type_python_interpreter_config_use_environment:

``use_environment``
^^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyPreConfig.use_environment <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.use_environment>`_.

.. _config_type_python_interpreter_config_utf8_mode:

``utf8_mode``
^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyPreConfig.utf8_mode <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.utf8_mode>`_.

.. _config_type_python_interpreter_config_pyconfig:

Attributes From ``PyConfig``
----------------------------

Attributes in this section correspond to fields of the
`PyConfig <https://docs.python.org/3/c-api/init_config.html#c.PyConfig>`_
C struct used to initialize the Python interpreter.

.. _config_type_python_interpreter_config_base_exec_prefix:

``base_exec_prefix``
^^^^^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.base_exec_prefix <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_exec_prefix>`_.

.. _config_type_python_interpreter_config_base_executable:

``base_executable``
^^^^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.base_exectuable <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_executable>`_.

.. _config_type_python_interpreter_config_base_prefix:

``base_prefix``
^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.base_prefix <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_prefix>`_.

.. _config_type_python_interpreter_config_buffered_stdio:

``buffered_stdio``
^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.buffered_stdio <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.buffered_stdio>`_.

.. _config_type_python_interpreter_config_bytes_warning:

``bytes_warning``
^^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.bytes_warning <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.bytes_warning>`_.

Accepted values are:

* ``None``
* ``none``
* ``warn``
* ``raise``

.. _config_type_python_interpreter_config_check_hash_pycs_mode:

``check_hash_pycs_mode``
^^^^^^^^^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.check_hash_pycs_mode <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.check_hash_pycs_mode>`_.

Accepted values are:

* ``None``
* ``always``
* ``never``
* ``default``

.. _config_type_python_interpreter_config_configure_c_stdio:

``configure_c_stdio``
^^^^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.configure_c_stdio <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.configure_c_stdio>`_.

.. _config_type_python_interpreter_config_dump_refs:

``dump_refs``
^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.dump_refs <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.dump_refs>`_.

.. _config_type_python_interpreter_config_exec_prefix:

``exec_prefix``
^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.exec_prefix <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.exec_prefix>`_.

.. _config_type_python_interpreter_config_executable:

``executable``
^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.executable <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.executable>`_.

.. _config_type_python_interpreter_config_fault_handler:

``fault_handler``
^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.fault_handler <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.fault_handler>`_.

.. _config_type_python_interpreter_config_filesystem_encoding:

``filesystem_encoding``
^^^^^^^^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.filesystem_encoding <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.filesystem_encoding>`_.

.. _config_type_python_interpreter_config_filesystem_errors:

``filesystem_errors``
^^^^^^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.filesystem_errors <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.filesystem_errors>`_.

.. _config_type_python_interpreter_config_hash_seed:

``hash_seed``
^^^^^^^^^^^^^

(``int`` or ``None``)

Controls the value of
`PyConfig.hash_seed <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.hash_seed>`_.

``PyConfig.use_hash_seed`` will automatically be set if this attribute is
defined.

.. _config_type_python_interpreter_config_home:

``home``
^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.home <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.home>`_.

.. _config_type_python_interpreter_config_import_time:

``import_time``
^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.import_time <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.import_time>`_.

.. _config_type_python_interpreter_config_inspect:

``inspect``
^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.inspect <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.inspect>`_.

.. _config_type_python_interpreter_config_install_signal_handlers:

``install_signal_handlers``
^^^^^^^^^^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.install_signal_handlers <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.install_signal_handlers>`_.

.. _config_type_python_interpreter_config_interactive:

``interactive``
^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.interactive <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.interactive>`_.

.. _config_type_python_interpreter_config_legacy_windows_stdio:

``legacy_windows_stdio``
^^^^^^^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.legacy_windows_stdio <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.legacy_windows_stdio>`_.

.. _config_type_python_interpreter_config_malloc_stats:

``malloc_stats``
^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.malloc_stats <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.malloc_stats>`_.

.. _config_type_python_interpreter_config_module_search_paths:

``module_search_paths``
^^^^^^^^^^^^^^^^^^^^^^^

(``list[string]`` or ``None``)

Controls the value of
`PyConfig.module_search_paths <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.module_search_paths>`_.

This value effectively controls the initial value of ``sys.path``.

The special string ``$ORIGIN`` in values will be expanded to the absolute
path of the directory of the executable at run-time. For example,
if the executable is ``/opt/my-application/pyapp``, ``$ORIGIN`` will
expand to ``/opt/my-application`` and the value ``$ORIGIN/lib`` will
expand to ``/opt/my-application/lib``.

Setting this to a non-empty value also has the side-effect of setting
``filesystem_importer = True``

.. _config_type_python_interpreter_config_optimization_level:

``optimization_level``
^^^^^^^^^^^^^^^^^^^^^^

(``int`` or ``None``)

Controls the value of
`PyConfig.optimization_level <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.optimization_level>`_.

Allowed values are:

* ``None``
* ``0``
* ``1``
* ``2``

This setting is only relevant if ``write_bytecode`` is ``True`` and
Python modules are being imported from the filesystem using Python's
standard filesystem importer.

.. _config_type_python_interpreter_config_parser_debug:

``parser_debug``
^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.parser_debug <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.parser_debug>`_.

.. _config_type_python_interpreter_config_pathconfig_warnings:

``pathconfig_warnings``
^^^^^^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.pathconfig_warnings <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pathconfig_warnings>`_.

.. _config_type_python_interpreter_config_prefix:

``prefix``
^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.prefix <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.prefix>`_.

.. _config_type_python_interpreter_config_program_name:

``program_name``
^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.program_name <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.program_name>`_.

.. _config_type_python_interpreter_config_pycache_prefix:

``pycache_prefix``
^^^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.pycache_prefix <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pycache_prefix>`_.

.. _config_type_python_interpreter_config_python_path_env:

``python_path_env``
^^^^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.pythonpath_env <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pythonpath_env>`_.

.. _config_type_python_interpreter_config_quiet:

``quiet``
^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.quiet <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.quiet>`_.

.. _config_type_python_interpreter_config_run_command:

``run_command``
^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.run_command <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_command>`_.

.. _config_type_python_interpreter_config_run_filename:

``run_filename``
^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.run_filename <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_filename>`_.

.. _config_type_python_interpreter_config_run_module:

``run_module``
^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.run_module <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_module>`_.

.. _config_type_python_interpreter_config_show_ref_count:

``show_ref_count``
^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.show_ref_count <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.show_ref_count>`_.

.. _config_type_python_interpreter_config_site_import:

``site_import``
^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.site_import <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.site_import>`_.

The ``site`` module is typically not needed for standalone/isolated Python
applications.

.. _config_type_python_interpreter_config_skip_first_source_line:

``skip_first_source_line``
^^^^^^^^^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.skip_first_source_line <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.skip_first_source_line>`_.

.. _config_type_python_interpreter_config_stdio_encoding:

``stdio_encoding``
^^^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.stdio_encoding <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.stdio_encoding>`_.

.. _config_type_python_interpreter_config_stdio_errors:

``stdio_errors``
^^^^^^^^^^^^^^^^

(``string`` or ``None``)

Controls the value of
`PyConfig.stdio_errors <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.stdio_errors>`_.

.. _config_type_python_interpreter_config_tracemalloc:

``tracemalloc``
^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.tracemalloc <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.tracemalloc>`_.

.. _config_type_python_interpreter_config_user_site_directory:

``user_site_directory``
^^^^^^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.user_site_directory <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.user_site_directory>`_.

.. _config_type_python_interpreter_config_verbose:

``verbose``
^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.verbose <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.verbose>`_.

.. _config_type_python_interpreter_config_warn_options:

``warn_options``
^^^^^^^^^^^^^^^^

(``list[string]`` or ``None``)

Controls the value of
`PyConfig.warn_options <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.warn_options>`_.

.. _config_type_python_interpreter_config_write_bytecode:

``write_bytecode``
^^^^^^^^^^^^^^^^^^

(``bool`` or ``None``)

Controls the value of
`PyConfig.write_bytecode <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.write_bytecode>`_.

This only influences the behavior of Python standard path-based importer
(controlled via ``filesystem_importer``).

.. _config_type_python_interpreter_config_x_options:

``x_options``
^^^^^^^^^^^^^^

(``list[string]`` or ``None``)

Controls the value of
`PyConfig.xoptions <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.xoptions>`_.

Starlark Caveats
================

The ``PythonInterpreterConfig`` Starlark type is backed by a Rust data
structure. And when attributes are retrieved, a copy of the underlying
Rust struct field is returned.

This means that if you attempt to mutate a Starlark value (as opposed to
assigning an attribute), the mutation won't be reflected on the underlying
Rust data structure.

For example:

.. code-block:: python

   config = dist.make_python_interpreter_config()

   # assigns vec!["foo", "bar"].
   config.module_search_paths = ["foo", "bar"]

   # Creates a copy of the underlying list and appends to that copy.
   # The stored value of `module_search_paths` is still `["foo", "bar"]`.
   config.module_search_paths.append("baz")

To append to a list, do something like the following:

.. code-block:: python

   value = config.module_search_paths
   value.append("baz")
   config.module_search_paths = value
