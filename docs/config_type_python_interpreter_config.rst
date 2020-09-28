.. _config_type_python_interpreter_config:

===========================
``PythonInterpreterConfig``
===========================

This type configures the default behavior of the embedded Python interpreter.

Embedded Python interpreters are configured and instantiated using a
Rust ``pyembed::PythonConfig`` data structure. The ``pyembed`` crate defines a
default instance of this data structure with parameters defined by the settings
in this type.

.. note::

   If you are writing custom Rust code and constructing a custom
   ``pyembed::PythonConfig`` instance and don't use the default instance, this
   config type is not relevant to you and can be omitted from your config
   file.

Constructors
============

.. _config_python_interpreter_config_init:

``PythonInterpreterConfig()``
-----------------------------

The ``PythonInterpreterConfig()`` constructor function can be called to
create a new instance of this type.

The following arguments can be defined to control the default ``PythonConfig``
behavior:

``bytes_warning`` (int)
   Controls the value of
   `Py_BytesWarningFlag <https://docs.python.org/3/c-api/init.html#c.Py_BytesWarningFlag>`_.

   Default is ``0``.

``filesystem_importer`` (bool)
   Controls whether to enable Python's filesystem based importer. Enabling
   this importer allows Python modules to be imported from the filesystem.

   Default is ``False`` (since PyOxidizer prefers embedding Python modules in
   binaries).

``ignore_environment`` (bool)
   Controls the value of
   `Py_IgnoreEnvironmentFlag <https://docs.python.org/3/c-api/init.html#c.Py_IgnoreEnvironmentFlag>`_.

   This is likely wanted for embedded applications that don't behave like
   ``python`` executables.

   Default is ``True``.

``inspect`` (bool)
   Controls the value of
   `Py_InspectFlag <https://docs.python.org/3/c-api/init.html#c.Py_InspectFlag>`_.

   Default is ``False``.

``interactive`` (bool)
   Controls the value of
   `Py_InteractiveFlag <https://docs.python.org/3/c-api/init.html#c.Py_InspectFlag>`_.

   Default is ``False``.

``isolated`` (bool)
   Controls the value of
   `Py_IsolatedFlag <https://docs.python.org/3/c-api/init.html#c.Py_IsolatedFlag>`_.

``legacy_windows_fs_encoding`` (bool)
   Controls the value of
   `Py_LegacyWindowsFSEncodingFlag <https://docs.python.org/3/c-api/init.html#c.Py_LegacyWindowsFSEncodingFlag>`_.

   Only affects Windows.

   Default is ``False``.

``legacy_windows_stdio`` (bool)
   Controls the value of
   `Py_LegacyWindowsStdioFlag <https://docs.python.org/3/c-api/init.html#c.Py_LegacyWindowsStdioFlag>`_.

   Only affects Windows.

   Default is ``False``.

``optimize_level`` (bool)
   Controls the value of
   `Py_OptimizeFlag <https://docs.python.org/3/c-api/init.html#c.Py_OptimizeFlag>`_.

   Default is ``0``, which is the Python default. Only the values ``0``, ``1``,
   and ``2`` are accepted.

   This setting is only relevant if ``write_bytecode`` is ``true`` and Python
   modules are being imported from the filesystem.

``parser_debug`` (bool)
   Controls the value of
   `Py_DebugFlag <https://docs.python.org/3/c-api/init.html#c.Py_DebugFlag>`_.

   Default is ``False``.

``quiet`` (bool)
   Controls the value of
   `Py_QuietFlag <https://docs.python.org/3/c-api/init.html#c.Py_QuietFlag>`_.

``raw_allocator`` (string)
   Which memory allocator to use for the ``PYMEM_DOMAIN_RAW`` allocator.

   This controls the lowest level memory allocator used by Python. All Python
   memory allocations use memory allocated by this allocator (higher-level
   allocators call into this pool to allocate large blocks then allocate
   memory out of those blocks instead of using the *raw* memory allocator).

   Values can be ``jemalloc``, ``rust``, or ``system``.

   ``jemalloc`` will have Python use the jemalloc allocator directly.

   ``rust`` will use Rust's global allocator (whatever that may be).

   ``system`` will use the default allocator functions exposed to the binary
   (``malloc()``, ``free()``, etc).

   The ``jemalloc`` allocator requires the ``jemalloc-sys`` crate to be
   available. A run-time error will occur if ``jemalloc`` is configured but this
   allocator isn't available.

   **Important**: the ``rust`` crate is not recommended because it introduces
   performance overhead.

   Default is ``jemalloc`` on non-Windows targets and ``system`` on Windows.
   (The ``jemalloc-sys`` crate doesn't work on Windows MSVC targets.)

``run_eval`` (string)
   Will cause the interpreter to evaluate a Python code string defined by this
   value after the interpreter initializes.

   An example value would be ``import mymodule; mymodule.main()``.

``run_file`` (string)
   Will cause the interpreter to evaluate a file at the specified filename.

   The filename is resolved at run-time using whatever mechanisms the Python
   interpreter applies. i.e. this is little different from running
   ``python <path>``.

``run_module`` (string)
   The Python interpreter will load a Python module with this value's name
   as the ``__main__`` module and then execute that module.

   This mode is similar to ``python -m <module>`` but isn't exactly the same.
   ``python -m <module>`` has additional functionality, such as looking for
   the existence of a ``<module>.__main__`` module. PyOxidizer does not do
   this. The value of this argument will be the exact module name that is
   imported and run as ``__main__``.

``run_noop`` (bool)
   Instructs the Python interpreter to do nothing after initialization.

``run_repl`` (bool)
   The Python interpreter will launch an interactive Python REPL connected to
   stdio. This is similar to the default behavior of running a ``python``
   executable without any arguments.

``site_import`` (bool)
   Controls the inverse value of
   `Py_NoSiteFlag <https://docs.python.org/3/c-api/init.html#c.Py_NoSiteFlag>`_.

   The ``site`` module is typically not needed for standalone Python applications.

   Default is ``False``.

``stdio_encoding`` (string)
   Defines the encoding and error handling mode for Python's standard I/O
   streams (``sys.stdout``, etc). Values are of the form ``encoding:error`` e.g.
   ``utf-8:ignore`` or ``latin1-strict``.

   If defined, the ``Py_SetStandardStreamEncoding()`` function is called during
   Python interpreter initialization. If not, the Python defaults are used.

``sys_frozen`` (bool)
   Controls whether to set the ``sys.frozen`` attribute to ``True``. If
   ``false``, ``sys.frozen`` is not set.

   Default is ``False``.

``sys_meipass`` (bool)
   Controls whether to set the ``sys._MEIPASS`` attribute to the path of
   the executable.

   Setting this and ``sys_frozen`` to ``true`` will emulate the
   `behavior of PyInstaller <https://pyinstaller.readthedocs.io/en/v3.3.1/runtime-information.html>`_
   and could possibly help self-contained applications that are aware of
   PyInstaller also work with PyOxidizer.

   Default is ``False``.

``sys_paths`` (array of strings)
   Defines filesystem paths to be added to ``sys.path``.

   Setting this value will imply ``filesystem_importer = true``.

   The special token ``$ORIGIN`` in values will be expanded to the absolute
   path of the directory of the executable at run-time. For example,
   if the executable is ``/opt/my-application/pyapp``, ``$ORIGIN`` will
   expand to ``/opt/my-application`` and the value ``$ORIGIN/lib`` will
   expand to ``/opt/my-application/lib``.

   If defined in multiple sections, new values completely overwrite old
   values (values are not merged).

   Default is an empty array (``[]``).

.. _config_terminfo_resolution:

``terminfo_resolution`` (string)
   How the terminal information database (``terminfo``) should be configured.

   See :ref:`terminfo_database` for more about terminal databases.

   The value ``dynamic`` (the default) looks at the currently running
   operating system and attempts to do something reasonable. For example, on
   Debian based distributions, it will look for the ``terminfo`` database in
   ``/etc/terminfo``, ``/lib/terminfo``, and ``/usr/share/terminfo``, which is
   how Debian configures ``ncurses`` to behave normally. Similar behavior exists
   for other recognized operating systems. If the operating system is unknown,
   PyOxidizer falls back to looking for the ``terminfo`` database in well-known
   directories that often contain the database (like ``/usr/share/terminfo``).

   The value ``none`` indicates that no configuration of the ``terminfo``
   database path should be performed. This is useful for applications that
   don't interact with terminals. Using ``none`` can prevent some filesystem
   I/O at application startup.

   The value ``static`` indicates that a static path should be used for the
   path to the ``terminfo`` database. That path should be provided by the
   ``terminfo_dirs`` configuration option.

   ``terminfo`` is not used on Windows and this setting is ignored on that
   platform.

``terminfo_dirs``
   Path to the ``terminfo`` database. See the above documentation for
   ``terminfo_resolution`` for more on the ``terminfo`` database.

   This value consists of a ``:`` delimited list of filesystem paths that
   ``ncurses`` should be configured to use. This value will be used to
   populate the ``TERMINFO_DIRS`` environment variable at application run time.

``unbuffered_stdio`` (bool)
   Controls the value of
   `Py_UnbufferedStdioFlag <https://docs.python.org/3/c-api/init.html#c.Py_UnbufferedStdioFlag>`_.

   Setting this makes the standard I/O streams unbuffered.

   Default is ``False``.

``use_hash_seed`` (bool)
   Controls the value of
   `Py_HashRandomizationFlag <https://docs.python.org/3/c-api/init.html#c.Py_HashRandomizationFlag>`_.

   Default is ``False``.

``user_site_directory`` (bool)
   Controls the inverse value of
   `Py_NoUserSiteDirectory <https://docs.python.org/3/c-api/init.html#c.Py_NoUserSiteDirectory>`_.

   Default is ``False``.

``write_bytecode`` (bool)
   Controls the inverse value of
   `Py_DontWriteBytecodeFlag <https://docs.python.org/3/c-api/init.html#c.Py_DontWriteBytecodeFlag>`_.

   This is only relevant if the interpreter is configured to import modules
   from the filesystem.

   Default is ``False``.

``write_modules_directory_env`` (string)
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
