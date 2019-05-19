===============
Technical Notes
===============

CPython Initialization
======================

Most code lives in ``pylifecycle.c``.

Call tree with Python 3.7::

    ``Py_Initialize()``
      ``Py_InitializeEx()``
        ``_Py_InitializeFromConfig(_PyCoreConfig config)``
          ``_Py_InitializeCore(PyInterpreterState, _PyCoreConfig)``
            Sets up allocators.
            ``_Py_InitializeCore_impl(PyInterpreterState, _PyCoreConfig)``
              Does most of the initialization.
              Runtime, new interpreter state, thread state, GIL, built-in types,
              Initializes sys module and sets up sys.modules.
              Initializes builtins module.
              ``_PyImport_Init()``
                Copies ``interp->builtins`` to ``interp->builtins_copy``.
              ``_PyImportHooks_Init()``
                Sets up ``sys.meta_path``, ``sys.path_importer_cache``,
                ``sys.path_hooks`` to empty data structures.
              ``initimport()``
                ``PyImport_ImportFrozenModule("_frozen_importlib")``
                ``PyImport_AddModule("_frozen_importlib")``
                ``interp->importlib = importlib``
                ``interp->import_func = interp->builtins.__import__``
                ``PyInit__imp()``
                  Initializes ``_imp`` module, which is implemented in C.
                ``sys.modules["_imp"} = imp``
                ``importlib._install(sys, _imp)``
                ``_PyImportZip_Init()``

          ``_Py_InitializeMainInterpreter(interp, _PyMainInterpreterConfig)``
            ``_PySys_EndInit()``
              ``sys.path = XXX``
              ``sys.executable = XXX``
              ``sys.prefix = XXX``
              ``sys.base_prefix = XXX``
              ``sys.exec_prefix = XXX``
              ``sys.base_exec_prefix = XXX``
              ``sys.argv = XXX``
              ``sys.warnoptions = XXX``
              ``sys._xoptions = XXX``
              ``sys.flags = XXX``
              ``sys.dont_write_bytecode = XXX``
            ``initexternalimport()``
              ``interp->importlib._install_external_importers()``
            ``initfsencoding()``
              ``_PyCodec_Lookup(Py_FilesystemDefaultEncoding)``
                ``_PyCodecRegistry_Init()``
                  ``interp->codec_search_path = []``
                  ``interp->codec_search_cache = {}``
                  ``interp->codec_error_registry = {}``
                  # This is the first non-frozen import during startup.
                  ``PyImport_ImportModuleNoBlock("encodings")``
                ``interp->codec_search_cache[codec_name]``
                ``for p in interp->codec_search_path: p[codec_name]``
            ``initsigs()``
            ``add_main_module()``
              ``PyImport_AddModule("__main__")``
            ``init_sys_streams()``
              ``PyImport_ImportModule("encodings.utf_8")``
              ``PyImport_ImportModule("encodings.latin_1")``
              ``PyImport_ImportModule("io")``
              Consults ``PYTHONIOENCODING`` and gets encoding and error mode.
              Sets up ``sys.__stdin__``, ``sys.__stdout__``, ``sys.__stderr__``.
            Sets warning options.
            Sets ``_PyRuntime.initialized``, which is what ``Py_IsInitialized()``
            returns.
            ``initsite()``
              ``PyImport_ImportModule("site")``

CPython Importing Mechanism
===========================

``Lib/importlib`` defines importing mechanisms and is 100% Python.

``Programs/_freeze_importlib.c`` is a program that takes a path to an input
``.py`` file and path to output ``.h`` file. It initializes a Python interpreter
and compiles the ``.py`` file to marshalled bytecode. It writes out a ``.h``
file with an inline ``const unsigned char _Py_M__importlib`` array containing
bytecode.

``Lib/importlib/_bootstrap_external.py`` compiled to
``Python/importlib_external.h`` with ``_Py_M__importlib_external[]``.

``Lib/importlib/_bootstrap.py`` compiled to
``Python/importlib.h`` with ``_Py_M__importlib[]``.

``Python/frozen.c`` has ``_PyImport_FrozenModules[]`` effectively mapping
``_frozen_importlib`` to ``importlib._bootstrap`` and
``_frozen_importlib_external`` to ``importlib._bootstrap_external``.

``initimport()`` calls ``PyImport_ImportFrozenModule("_frozen_importlib")``,
effectively ``import importlib._bootstrap``. Module import doesn't appear
to have meaningful side-effects.

``importlib._bootstrap.__import__`` is installed as ``interp->import_func``.

C implemented ``_imp`` module is initialized.

``importlib._bootstrap._install(sys, _imp`` is called. Calls
``_setup(sys, _imp)`` and adds ``BuiltinImporter`` and ``FrozenImporter``
to ``sys.meta_path``.

``_setup()`` defines globals ``_imp`` and ``sys``. Populates ``__name__``,
``__loader__``, ``__package__``, ``__spec__``, ``__path__``, ``__file__``,
``__cached__`` on all ``sys.modules`` entries. Also loads builtins
``_thread``, ``_warnings``, and ``_weakref``.

Later during interpreter initialization, ``initexternal()`` effectively calls
``importlib._bootstrap._install_external_importers()``. This runs
``import _frozen_importlib_external``, which is effectively
``import importlib._bootstrap_external``. This module handle is aliased to
``importlib._bootstrap._bootstrap_external``.

``importlib._bootstrap_external`` import doesn't appear to have significant
side-effects.

``importlib._bootstrap_external._install()`` is called with a reference to
``importlib._bootstrap``. ``_setup()`` is called.

``importlib._bootstrap._setup()`` imports builtins ``_io``, ``_warnings``,
``_builtins``, ``marshal``. Either ``posix`` or ``nt`` imported depending
on OS. Various module-level attributes set defining run-time environment.
This includes ``_winreg``. ``SOURCE_SUFFIXES`` and ``EXTENSION_SUFFIXES``
are updated accordingly.

``importlib._bootstrap._get_supported_file_loaders()`` returns various
loaders. ``ExtensionFileLoader`` configured from ``_imp.extension_suffixes()``.
``SourceFileLoader`` configured from ``SOURCE_SUFFIXES``.
``SourcelessFileLoader`` configured from ``BYTECODE_SUFFIXES``.

``FileFinder.path_hook()`` called with all loaders and result added to
``sys.path_hooks``. ``PathFinder`` added to ``sys.meta_path``.

``sys.modules`` After Interpreter Init
======================================

============================== ========== ================================
Module                         Type       Source
============================== ========== ================================
``__main__``                              ``add_main_module()``
``_abc``                       builtin    ``abc``
``_codecs``                    builtin    ``initfsencoding()``
``_frozen_importlib``          frozen     ``initimport()``
``_frozen_importlib_external`` frozen     ``initexternal()``
``_imp``                       builtin    ``initimport()``
``_io``                        builtin    ``importlib._bootstrap._setup()``
``_signal``                    builtin    ``initsigs()``
``_thread``                    builtin    ``importlib._bootstrap._setup()``
``_warnings``                  builtin    ``importlib._bootstrap._setup()``
``_weakref``                   builtin    ``importlib._bootstrap._setup()``
``_winreg``                    builtin    ``importlib._bootstrap._setup()``
``abc``                        py
``builtins``                   builtin    ``_Py_InitializeCore_impl()``
``codecs``                     py         ``encodings`` via ``initfsencoding()``
``encodings``                  py         ``initfsencoding()``
``encodings.aliases``          py         ``encodings``
``encodings.latin_1``          py         ``init_sys_streams()``
``encodings.utf_8``            py         ``init_sys_streams()`` + ``initfsencoding()``
``io``                         py         ``init_sys_streams()``
``marshal``                    builtin    ``importlib._bootstrap._setup()``
``nt``                         builtin    ``importlib._bootstrap._setup()``
``posix``                      builtin    ``importlib._bootstrap._setup()``
``readline``                   builtin
``sys``                        builtin    ``_Py_InitializeCore_impl()``
``zipimport``                  builtin    ``initimport()``
============================== ========== =================================

Modules Imported by ``site.py``
===============================

``_collections_abc``
``_sitebuiltins``
``_stat``
``atexit``
``genericpath``
``os``
``os.path``
``posixpath``
``rlcompleter``
``site``
``stat``

Random Notes
============

Frozen importer iterates an array looking for module names. On each item, it
calls ``_PyUnicode_EqualToASCIIString()``, which verifies the search name is
ASCII. Performing an O(n) scan for every frozen module if there are a large
number of frozen modules could contribute performance overhead. A better frozen
importer would use a map/hash/dict for lookups. This //may// require CPython
API breakages, as the ``PyImport_FrozenModules`` data structure is documented
as part of the public API and its value could be updated dynamically at
run-time.

``importlib._bootstrap`` cannot call ``import`` because the global import
hook isn't registered until after ``initimport()``.

``importlib._bootstrap_external`` is the best place to monkeypatch because
of the limited run-time functionality available during ``importlib._bootstrap``.

It's a bit wonky that ``Py_Initialize()`` will import modules from the
standard library and it doesn't appear possible to disable this. If
``site.py`` is disabled, non-extension builtins are limited to
``codecs``, ``encodings``, ``abc``, and whatever ``encodings.*`` modules
are needed by ``initfsencoding()`` and ``init_sys_streams()``.

An attempt was made to freeze the set of standard library modules loaded
during initialization. However, the built-in extension importer doesn't
set all of the module attributes that are expected of the modules system.
The ``from . import aliases`` in ``encodings/__init__.py`` is confused
without these attributes. And relative imports seemed to have issues as
well. One would think it would be possible to run an embedded interpreter
with all standard library modules frozen, but this doesn't work.
