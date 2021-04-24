.. py:currentmodule:: starlark_pyoxidizer

===========================
``PythonInterpreterConfig``
===========================

.. py:class:: PythonInterpreterConfig

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

    Instances are constructed by calling
    :py:meth:`PythonDistribution.make_python_interpreter_config`.

    Instance state is managed via attributes.

    There are a ton of attributes and most attributes are not relevant
    to most applications. The bulk of the attributes exist to give full
    control over Python interpreter initialization.

    The following attributes control features provided by the ``pyembed`` Rust crate,
    which manages the embedded Python interpreter in generated executables.
    These attributes provide features and level of control over
    embedded Python interpreters beyond what is possible with Python's
    `initialization C API <https://docs.python.org/3/c-api/init_config.html>`_.

    * :py:attr:`allocator_backend`
    * :py:attr:`allocator_raw`
    * :py:attr:`allocator_mem`
    * :py:attr:`allocator_obj`
    * :py:attr:`allocator_pymalloc_arena`
    * :py:attr:`allocator_debug`
    * :py:attr:`oxidized_importer`
    * :py:attr:`filesystem_importer`
    * :py:attr:`argvb`
    * :py:attr:`sys_frozen`
    * :py:attr:`sys_meipass`
    * :py:attr:`terminfo_resolution`
    * :py:attr:`write_modules_directory_env`

    The following attributes correspond to fields of the
    `PyPreConfig <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig>`_
    C struct used to initialize the Python interpreter.

    * :py:attr:`config_profile`
    * :py:attr:`allocator`
    * :py:attr:`configure_locale`
    * :py:attr:`coerce_c_locale`
    * :py:attr:`coerce_c_locale_warn`
    * :py:attr:`development_mode`
    * :py:attr:`isolated`
    * :py:attr:`legacy_windows_fs_encoding`
    * :py:attr:`parse_argv`
    * :py:attr:`use_environment`
    * :py:attr:`utf8_mode`

    The following attributes correspond to fields of the
    `PyConfig <https://docs.python.org/3/c-api/init_config.html#c.PyConfig>`_
    C struct used to initialize the Python interpreter.

    * :py:attr:`base_exec_prefix`
    * :py:attr:`base_executable`
    * :py:attr:`base_prefix`
    * :py:attr:`buffered_stdio`
    * :py:attr:`bytes_warning`
    * :py:attr:`check_hash_pycs_mode`
    * :py:attr:`configure_c_stdio`
    * :py:attr:`dump_refs`
    * :py:attr:`exec_prefix`
    * :py:attr:`executable`
    * :py:attr:`fault_handler`
    * :py:attr:`filesystem_encoding`
    * :py:attr:`hash_seed`
    * :py:attr:`home`
    * :py:attr:`import_time`
    * :py:attr:`inspect`
    * :py:attr:`install_signal_handlers`
    * :py:attr:`interactive`
    * :py:attr:`legacy_windows_stdio`
    * :py:attr:`malloc_stats`
    * :py:attr:`module_search_paths`
    * :py:attr:`optimization_level`
    * :py:attr:`parser_debug`
    * :py:attr:`pathconfig_warnings`
    * :py:attr:`prefix`
    * :py:attr:`program_name`
    * :py:attr:`pycache_prefix`
    * :py:attr:`python_path_env`
    * :py:attr:`quiet`
    * :py:attr:`run_command`
    * :py:attr:`run_filename`
    * :py:attr:`run_module`
    * :py:attr:`show_ref_count`
    * :py:attr:`site_import`
    * :py:attr:`skip_first_source_line`
    * :py:attr:`stdio_encoding`
    * :py:attr:`stdio_errors`
    * :py:attr:`tracemalloc`
    * :py:attr:`user_site_directory`
    * :py:attr:`verbose`
    * :py:attr:`warn_options`
    * :py:attr:`write_bytecode`
    * :py:attr:`x_options`

    .. py:attribute:: allocator_backend

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

    .. py:attribute:: allocator_raw

        (``bool``)

        Controls whether to install a custom allocator (defined by
        ``allocator_backend``) into Python's *raw* allocator domain
        (``PYMEM_DOMAIN_RAW`` in Python C API speak).

        Setting this to ``True`` will replace the system allocator (e.g. ``malloc()``,
        ``free()``) for this domain.

        A value of ``True`` only has an effect if ``allocator_backend`` is some value
        other than ``default``.

        Defaults to ``True``.

    .. py:attribute:: allocator_mem

        (``bool``)

        Controls whether to install a custom allocator (defined by
        ``allocator_backend``) into Python's *mem* allocator domain
        (``PYMEM_DOMAIN_MEM`` in Python C API speak).

        Setting this to ``True`` will replace ``pymalloc`` as the allocator
        for this domain.

        A value of ``True`` only has an effect if ``allocator_backend`` is some value
        other than ``default``.

        Defaults to ``False``.

    .. py:attribute:: allocator_obj

        (``bool``)

        Controls whether to install a custom allocator (defined by
        ``allocator_backend``) into Python's *obj* allocator domain
        (``PYMEM_DOMAIN_OBJ`` in Python C API speak).

        Setting this to ``True`` will replace ``pymalloc`` as the allocator
        for this domain.

        A value of ``True`` only has an effect if ``allocator_backend`` is some value
        other than ``default``.

        Defaults to ``False``.

    .. py:attribute:: allocator_pymalloc_arena

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

    .. py:attribute:: allocator_debug

        (``bool``)

        Whether to enable debug hooks for Python's memory allocators.

        Enabling debug hooks enables debugging of memory-related issues in the
        Python interpreter. This setting effectively controls whether to call
        `PyMem_SetupDebugHooks() <https://docs.python.org/3/c-api/memory.html#c.PyMem_SetupDebugHooks>`_
        during interpreter initialization. See the linked documentation for more.

        Defaults to ``False``.

    .. py:attribute:: oxidized_importer

        (``bool``)

        Whether to install the ``oxidized_importer`` meta path importer
        (:ref:`oxidized_importer`) on ``sys.meta_path`` and ``sys.path_hooks`` during
        interpreter initialization. If installed, we will always occupy the
        first element in these lists.

        Defaults to ``True``.

    .. py:attribute:: filesystem_importer

        (``bool``)

        Whether to install the standard library path-based importer for
        loading Python modules from the filesystem.

        If disabled, ``sys.meta_path`` and ``sys.path_hooks`` will not have
        entries provided by the standard library's path-based importer.

        Due to quirks in how the Python interpreter is initialized, the standard
        library's path-based importer will be registered on ``sys.meta_path``
        and ``sys.path_hooks`` for a brief moment when the interpreter is
        initialized. If ``sys.path`` contains valid entries that would be
        serviced by this importer and ``oxidized_importer`` isn't able to
        service imports, it is possible for the path-based importer to be
        used to import some Python modules needed to initialize the Python
        interpreter. In many cases, this behavior is harmless. In all cases,
        the path-based importer is disabled after Python interpreter
        initialization, so future imports won't be serviced by the
        path-based importer if it is disabled by this flag.

        The filesystem importer is enabled automatically if
        :py:attr:`PythonInterpreterConfig.module_search_paths` is non-empty.

    .. py:attribute:: argvb

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

    .. py:attribute:: sys_frozen

        (``bool``)

        Controls whether to set the ``sys.frozen`` attribute to ``True``. If
        ``false``, ``sys.frozen`` is not set.

        Default is ``False``.

    .. py:attribute:: sys_meipass

        (``bool``)

        Controls whether to set the ``sys._MEIPASS`` attribute to the path of
        the executable.

        Setting this and ``sys_frozen`` to ``True`` will emulate the
        `behavior of PyInstaller <https://pyinstaller.readthedocs.io/en/v3.3.1/runtime-information.html>`_
        and could possibly help self-contained applications that are aware of
        PyInstaller also work with PyOxidizer.

        Default is ``False``.

    .. py:attribute:: terminfo_resolution

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

    .. py:attribute:: write_modules_directory_env

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

    .. py:attribute:: config_profile

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

    .. py:attribute:: allocator

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

    .. py:attribute:: configure_locale

        (``bool`` or ``None``)

        Controls the value of
        `PyPreConfig.configure_locale <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.configure_locale>`_.

    .. py:attribute:: coerce_c_locale

        (``string`` or ``None``)

        Controls the value of
        `PyPreConfig.coerce_c_locale <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale>`_.

        Accepted values are:

        ``LC_CTYPE``
           Read ``LC_CTYPE``

        ``C``
           Coerce the ``C`` locale.

    .. py:attribute:: coerce_c_locale_warn

        (``bool`` or ``None``)

        Controls the value of
        `PyPreConfig.coerce_c_locale_warn <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale_warn>`_.

    .. py:attribute:: development_mode

        (``bool`` or ``None``)

        Controls the value of
        `PyPreConfig.development_mode <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.development_mode>`_.

    .. py:attribute:: isolated

        (``bool`` or ``None``)

        Controls the value of
        `PyPreConfig.isolated <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.isolated>`_.

    .. py:attribute:: legacy_windows_fs_encoding

        (``bool`` or ``None``)

        Controls the value of
        `PyPreConfig.legacy_windows_fs_encoding <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.legacy_windows_fs_encoding>`_.

    .. py:attribute:: parse_argv

        (``bool`` or ``None``)

        Controls the value of
        `PyPreConfig.parse_argv <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.parse_argv>`_.

    .. py:attribute:: use_environment

        (``bool`` or ``None``)

        Controls the value of
        `PyPreConfig.use_environment <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.use_environment>`_.

    .. py:attribute:: utf8_mode

        (``bool`` or ``None``)

        Controls the value of
        `PyPreConfig.utf8_mode <https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.utf8_mode>`_.

    .. py:attribute:: base_exec_prefix

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.base_exec_prefix <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_exec_prefix>`_.

    .. py:attribute:: base_executable

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.base_exectuable <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_executable>`_.

    .. py:attribute:: base_prefix

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.base_prefix <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_prefix>`_.

    .. py:attribute:: buffered_stdio

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.buffered_stdio <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.buffered_stdio>`_.

    .. py:attribute:: bytes_warning

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.bytes_warning <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.bytes_warning>`_.

        Accepted values are:

        * ``None``
        * ``none``
        * ``warn``
        * ``raise``

    .. py:attribute:: check_hash_pycs_mode

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.check_hash_pycs_mode <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.check_hash_pycs_mode>`_.

        Accepted values are:

        * ``None``
        * ``always``
        * ``never``
        * ``default``

    .. py:attribute:: configure_c_stdio

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.configure_c_stdio <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.configure_c_stdio>`_.

    .. py:attribute:: dump_refs

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.dump_refs <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.dump_refs>`_.

    .. py:attribute:: exec_prefix

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.exec_prefix <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.exec_prefix>`_.

    .. py:attribute:: executable

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.executable <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.executable>`_.

    .. py:attribute:: fault_handler

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.fault_handler <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.fault_handler>`_.

    .. py:attribute:: filesystem_encoding

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.filesystem_encoding <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.filesystem_encoding>`_.

    .. py:attribute:: filesystem_errors

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.filesystem_errors <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.filesystem_errors>`_.

    .. py:attribute:: hash_seed

        (``int`` or ``None``)

        Controls the value of
        `PyConfig.hash_seed <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.hash_seed>`_.

        ``PyConfig.use_hash_seed`` will automatically be set if this attribute is
        defined.

    .. py:attribute:: home

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.home <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.home>`_.

    .. py:attribute:: import_time

        Controls the value of
        `PyConfig.import_time <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.import_time>`_.

    .. py:attribute:: inspect

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.inspect <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.inspect>`_.

    .. py:attribute:: install_signal_handlers

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.install_signal_handlers <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.install_signal_handlers>`_.

    .. py:attribute:: interactive

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.interactive <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.interactive>`_.

    .. py:attribute:: legacy_windows_stdio

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.legacy_windows_stdio <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.legacy_windows_stdio>`_.

    .. py:attribute:: malloc_stats

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.malloc_stats <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.malloc_stats>`_.

    .. py:attribute:: module_search_paths

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

    .. py:attribute:: optimization_level

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

    .. py:attribute:: parser_debug

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.parser_debug <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.parser_debug>`_.

    .. py:attribute:: pathconfig_warnings

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.pathconfig_warnings <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pathconfig_warnings>`_.

    .. py:attribute:: prefix

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.prefix <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.prefix>`_.

    .. py:attribute:: program_name

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.program_name <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.program_name>`_.

    .. py:attribute:: pycache_prefix

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.pycache_prefix <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pycache_prefix>`_.

    .. py:attribute:: python_path_env

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.pythonpath_env <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pythonpath_env>`_.

    .. py:attribute:: quiet

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.quiet <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.quiet>`_.

    .. py:attribute:: run_command

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.run_command <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_command>`_.

    .. py:attribute:: run_filename

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.run_filename <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_filename>`_.

    .. py:attribute:: run_module

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.run_module <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_module>`_.

    .. py:attribute:: show_ref_count

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.show_ref_count <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.show_ref_count>`_.

    .. py:attribute:: site_import

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.site_import <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.site_import>`_.

        The ``site`` module is typically not needed for standalone/isolated Python
        applications.

    .. py:attribute:: skip_first_source_line

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.skip_first_source_line <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.skip_first_source_line>`_.

    .. py:attribute:: stdio_encoding

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.stdio_encoding <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.stdio_encoding>`_.

    .. py:attribute:: stdio_errors

        (``string`` or ``None``)

        Controls the value of
        `PyConfig.stdio_errors <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.stdio_errors>`_.

    .. py:attribute:: tracemalloc

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.tracemalloc <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.tracemalloc>`_.

    .. py:attribute:: user_site_directory

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.user_site_directory <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.user_site_directory>`_.

    .. py:attribute:: verbose

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.verbose <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.verbose>`_.

    .. py:attribute:: warn_options

        (``list[string]`` or ``None``)

        Controls the value of
        `PyConfig.warn_options <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.warn_options>`_.

    .. py:attribute:: write_bytecode

        (``bool`` or ``None``)

        Controls the value of
        `PyConfig.write_bytecode <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.write_bytecode>`_.

        This only influences the behavior of Python standard path-based importer
        (controlled via ``filesystem_importer``).

    .. py:attribute:: x_options

        (``list[string]`` or ``None``)

        Controls the value of
        `PyConfig.xoptions <https://docs.python.org/3/c-api/init_config.html#c.PyConfig.xoptions>`_.

Starlark Caveats
================

The :py:class:`PythonInterpreterConfig` Starlark type is backed by a Rust data
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
