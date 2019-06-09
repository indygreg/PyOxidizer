.. _config_files:

===================
Configuration Files
===================

PyOxidizer uses TOML configuration files to configure how Python is packaged
and built applications behave.

Finding Configuration Files
===========================

The TOML configuration file is processed as part of building the ``pyembed``
crate. This is the crate that manages an embedded Python interpreter in a
larger Rust project.

If the ``PYOXIDIZER_CONFIG`` environment variable is set, the path specified
by this environment variable will be used as the location of the TOML
configuration file.

If ``PYOXIDIZER_CONFIG`` is not set, the build will look for a
``pyoxidizer.toml`` starting in the directory of the ``pyembed`` crate and
then traversing ancestor directories until a file is found.

If no configuration file is found, an error occurs.

File Processing Semantics
=========================

The configuration file format is designed to be simultaneously used by multiple
build *targets*, where a target is a Rust toolchain target triple, such as
``x86_64-unknown-linux-gnu`` or ``x86_64-pc-windows-msvc``. Each TOML section
accepts an optional ``target`` key that can be used to control whether the
section is applied or ignored. If the ``target`` key is not defined or has
the special value ``all``, it is always applied. Otherwise the section is only
applied if its ``target`` value matches the Rust build target.

Unless specified otherwise, section processing works by first initializing
a section with defaults. As sections are read from the config file, if
the section is active for the current ``target``, any encountered keys are
set. The final set of values across all encountered sections is used.

Configuration Sections
======================

The following documentation sections describe the various TOML sections.

``[[python_distribution]]``
---------------------------

Defines a Python distribution that can be embedded into a binary.

A Python distribution is a zstandard-compressed tar archive containing a
specially produced build of Python. These distributions are typically
produced by the
`python-build-standalone <https://github.com/indygreg/python-build-standalone>`_
project. Pre-built distributions are available at
https://github.com/indygreg/python-build-standalone/releases.

The ``pyoxidizer`` binary has a set of known distributions built-in
which are automatically added to generated ``pyoxidizer.toml`` config files.
Typically you don't need to build your own distribution or change
the distribution manually: distributions are managed automatically
by ``pyoxidizer``.

A distribution is defined by a target triple, location, and a hash.

One of ``local_path`` or ``url`` MUST be defined.

``target`` (string)

   Target triple this distribution is compiled for.

``sha256`` (string)

   The SHA-256 of the distribution archive file.

``local_path`` (string)

   Local filesystem path to the distribution archive.

``url`` (string)

   URL from which a distribution archive can be obtained using an HTTP GET
   request.

Examples
^^^^^^^^

::

   [[python_distribution]]
   target = "x86_64-unknown-linux-gnu"
   local_path = "/var/python-distributions/cpython-linux64.tar.zst"
   sha256 = "11a53f5755773f91111a04f6070a6bc00518a0e8e64d90f58584abf02ca79081"

``[[python_config]]``
---------------------

This section configures the embedded Python interpreter.

Embedded Python interpreters are configured and instantiated using a
``pyembed::PythonConfig`` data structure. The ``pyembed`` crate defines a
default instance of this data structure with parameters defined by the settings
in this TOML section.

If you are constructing a custom ``pyembed::PythonConfig`` instance and don't
use the default instance, this config section is not relevant to you.

The following keys can be defined to control the default ``PythonConfig``
behavior:

``dont_write_bytecode`` (bool)

   Controls the value of
   `Py_DontWriteBytecodeFlag <https://docs.python.org/3/c-api/init.html#c.Py_DontWriteBytecodeFlag>`_.

   This is only relevant if the interpreter is configured to import modules
   from the filesystem.

   Default is ``true``.

``ignore_environment`` (bool)

   Controls the value of
   `Py_IgnoreEnvironmentFlag <https://docs.python.org/3/c-api/init.html#c.Py_IgnoreEnvironmentFlag>`_.

   This is likely wanted for embedded applications that don't behave like
   ``python`` executables.

   Default is ``true``.

``no_site`` (bool)

   Controls the value of
   `Py_NoSiteFlag <https://docs.python.org/3/c-api/init.html#c.Py_NoSiteFlag>`_.

   The ``site`` module is typically not needed for standalone Python applications.

   Default is ``true``.

``no_user_site_directory`` (bool)

   Controls the value of
   `Py_NoUserSiteDirectory <https://docs.python.org/3/c-api/init.html#c.Py_NoUserSiteDirectory>`_.

   Default is ``true``.

``optimize_level`` (bool)

   Controls the value of
   `Py_OptimizeFlag <https://docs.python.org/3/c-api/init.html#c.Py_OptimizeFlag>`_.

   Default is ``0``, which is the Python default. Only the values ``0``, ``1``,
   and ``2`` are accepted.

   This setting is only relevant if ``dont_write_bytecode`` is ``false`` and Python
   modules are being imported from the filesystem.

``program_name`` (string)

   The name of the running application. This value will be passed to
   ``Py_SetProgramName()``.

   Default value is the string ``undefined``.

``stdio_encoding`` (string)

   Defines the encoding and error handling mode for Python's standard I/O
   streams (``sys.stdout``, etc). Values are of the form ``encoding:error`` e.g.
   ``utf-8:ignore`` or ``latin1-strict``.

   If defined, the ``Py_SetStandardStreamEncoding()`` function is called during
   Python interpreter initialization. If not, the Python defaults are used.

``unbuffered_stdio`` (bool)

   Controls the value of
   `Py_UnbufferedStdioFlag <https://docs.python.org/3/c-api/init.html#c.Py_UnbufferedStdioFlag>`_.

   Setting this makes the standard I/O streams unbuffered.

   Default is ``false``.

``filesystem_importer`` (bool)

   Controls whether to enable Python's filesystem based importer. Enabling
   this importer allows Python modules to be imported from the filesystem.

   Default is ``false`` (since PyOxidizer prefers embedding Python modules in
   binaries).

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

   Default is ``jemalloc``.

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

``[[python_packages]]``
-----------------------

Defines a rule to control the packaging of Python resources to be embedded
in the binary.

A *Python resource* here can be one of the following:

* *Extension module*. An extension module is a Python module backed by compiled
  code (typically written in C).
* *Python module source*. A Python module's source code. This is typically the
  content of a ``.py`` file.
* *Python module bytecode*. A Python module's source compiled to Python
  bytecode. This is similar to a ``.pyc`` files but isn't exactly the same
  (``.pyc`` files have a header in addition to the raw bytecode).
* *Resource file*. Non-module files that can be accessed via APIs in Python's
  importing mechanism.

*Extension modules* are a bit special in that they can have library
dependencies. If an extension module has an annotated library dependency,
that library will automatically be linked into the produced binary containing
Python. Static linking is used, if available. For example, the ``_sqlite3``
extension module will link the ``libsqlite3`` library (which should be
included as part of the Python distribution).

Each entry of this section describes a specific rule for finding and
including or excluding resources. Each section has a ``type`` key
describing the *flavor* of rule this is.

When packaging goes to resolve the set of resources, it starts with an
empty set for each resource *flavor*. As sections are read, their results are
*merged* with the existing resource sets according to the behavior of that
rule ``type``. If multiple rules add a resource of the same name and flavor, the
last added version is used. i.e. *last write wins*.

The following sections describe the various ``type``'s of rules.

``stdlib-extension-policy``
^^^^^^^^^^^^^^^^^^^^^^^^^^^

This rule defines a base policy for what *extension modules* to include
from the Python distribution.

This type has a ``policy`` key denoting the *policy* to use. This key can have
the following values:

``minimal``
   Include the minimal set of extension modules required to initialize a
   Python interpreter. This is a very small set and various common
   functionality from the Python standard library will not work with this
   value.

``all``
   Includes all available extension modules in the Python distribution.

``no-libraries``
   Includes all available extension modules in the Python distribution that
   do not have an additional library dependency. Most common Python extension
   modules are included. Extension modules like ``_ssl`` (links against
   OpenSSL) and ``zlib`` are not included.

``no-gpl``
   Includes all available extension modules in the Python distribution that
   do not link against GPL licensed libraries.

   Not all Python distributions may annotate license info for all extensions or
   the libraries they link against. If license info is missing, the extension is
   not included because it *could* be GPL licensed. Similarly, the mechanism for
   determining whether a license is GPL is based on an explicit list of non-GPL
   licenses. This ensures new GPL licenses don't slip through.

Example::

   [[python_packages]]
   type = "stdlib-extension-policy"
   policy = "no-libraries"

.. important::

   Libraries that extension modules link against have various software
   licenses, including GPL version 3. Adding these extension modules will
   also include the library. This typically exposes your program to additional
   licensing requirements, including making your application subject to that
   license and therefore open source. See :ref:`licensing_considerations` for
   more.

``stdlib-extensions-explicit-includes``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This rule allows including explicitly delimited extension modules from
the Python distribution.

The section must define an ``includes`` key, which is an array of strings
of extension module names.

This policy is typically combined with the ``minimal`` ``stdlib-extension-policy``
to cherry pick individual extension modules for inclusion.

Example::

   [[python_packages]]
   type = "stdlib-extensions-explicit-includes"
   includes = ["binascii", "errno", "itertools", "math", "select", "_socket"]

``stdlib-extensions-explicit-excludes``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This rule allows excluding explicitly delimited extension modules from
the Python distribution.

The section must define an ``excludes`` key, which is an array of strings of
extension module names.

Example::

   [[python_packages]]
   type = "stdlib-extensions-explicit-excludes"
   excludes = ["_ssl"]

``stdlib-extension-variant``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This rule specifies the inclusion of a specific extension module *variant*.

Some Python distributions offer multiple variants for an individual extension
module. For example, the ``readline`` extension module may offer a ``libedit``
variant that is compiled against ``libedit`` instead of ``libreadline`` (the default).

By default, the first listed extension module variant in a Python distribution
is used. By defining rules of this type, one can use an alternate or explicit
extension module variation.

Extension module variants are defined the the ``extension`` and ``variant`` keys.
The former defines the extension module name. The latter its variant name.

Example::

   [[python_packages]]
   type = "stdlib-extension-variant"
   extension = "readline"
   variant = "libedit"

``stdlib``
^^^^^^^^^^

This rule controls packaging of non-extension modules Python resources from
the Python distribution's standard library. Presence of this rule will
pull in the Python standard library in its entirety.

.. important::

   A ``stdlib`` rule is required, as Python can't be initialized
   without some modules from the standard library. It should be one of the first
   ``[[python_packages]]`` entries so the standard library forms the base of the
   set of Python modules to include.

The following keys can exist in this rule type:

``exclude_test_modules`` (bool)

   Indicates whether test-only modules should be included in packaging. The
   Python standard library ships various packages and modules that are used for
   testing Python itself. These modules are not referenced by *real* modules
   in the Python standard library and can usually be safely excluded.

   Default is ``true``.

``optimize_level`` (int)

   The optimization level for packaged bytecode. Allowed values are ``0``, ``1``, and
   ``2``.

   Default is ``0``, which is the Python default.

``include_source`` (bool)

   Whether to include the source code for modules in addition to bytecode.

   Default is ``true``.

``include_resources`` (bool)

   Whether to include non-module resource files.

   These are files like ``lib2to3/Grammar.txt`` which are present in the
   standard library but aren't typically used for common functionality.

   Default is ``false``.

``package-root``
^^^^^^^^^^^^^^^^

This rule discovers resources from a directory on the filesystem.

The specified directory will be scanned for resource files. However,
only specific named *packages* will be packaged. e.g. if the directory
contains sub-directories ``foo/`` and ``bar``, you must explicitly
state that you want the ``foo`` and/or ``bar`` package to be included so files
from these directories will be included.

This rule is frequently used to pull in packages from local source
directories (e.g. directories containing a ``setup.py`` file). This
rule doesn't involve any packaging tools and is a purely driven by
filesystem walking. It is primitive, yet effective.

This rule has the following keys:

``path`` (string)

   The filesystem path to the directory to scan.

``optimize_level`` (int)

   The module optimization level for packaged bytecode.

   Allowed values are ``0``, ``1``, and ``2``.

   Defaults to ``0``, which is the Python default.

``packages`` (array of string)

   List of package names to include.

   Filesystem walking will find files in a directory ``<path>/<value>/`` or in
   a file ``<path>/<value>.py``.

``excludes`` (array of string)

   An array of package or module names to exclude.

   A value in this array will match on an exact full resource name match or
   on a package prefix match. e.g. ``foo`` will match the module ``foo``, the
   package ``foo``, and any sub-modules in ``foo``. e.g. it will match
   ``foo.bar`` but will not match ``foofoo``.

   Default is an empty array.

``include_source`` (bool)

   Whether to include the source code for modules in addition to the bytecode.

   Default is ``true``.

``pip-install-simple``
^^^^^^^^^^^^^^^^^^^^^^

This rule runs ``pip install`` for a single package and will automatically
package all Python resources associated with that operation, including
resources associated with dependencies.

Using this rule, one can easily add multiple Python packages with a single
rule.

``package`` (string)

   Name of the package to install. This is added as a positional argument to
   ``pip install``.

``optimize_level`` (int)

   The module optimization level for packaged bytecode.

   Allowed values are ``0``, ``1``, and ``2``.

   Default is ``0``, which is the Python default.

``include_source`` (bool)

   Whether to include the source code for Python modules in addition to
   the byte code.

   Default is ``true``.

This will include the ``pyflakes`` package and all its dependencies::

   [[python_packages]]
   type = "pip-install-simple"
   package = "pyflakes"

``pip-requirements-file``
^^^^^^^^^^^^^^^^^^^^^^^^^

This rule runs ``pip install -r <path>`` for a given
`pip requirements file <https://pip.pypa.io/en/stable/user_guide/#requirements-files>`_.
This allows multiple Python packages to be downloaded/installed in a single
operation.

``requirements_path`` (string)

   Filesystem path to pip requirements file.

``optimize_level`` (int)

   The module optimization level for packaged bytecode.

   Allowed values are ``0``, ``1``, and ``2``.

   Defaults to ``0``, which is the Python default.

``include_source`` (bool)

   Whether to include the source code for Python modules in addition to the
   bytecode.

   Default is ``true``.

Example::

   [[python_packages]]
   type = "pip-requirements-file"
   path = "/home/gps/src/myapp/requirements.txt"

``setup-py-install``
^^^^^^^^^^^^^^^^^^^^

This rule runs ``python setup.py install`` for a given directory containing a
``setup.py`` ``distutils``/``setuptools`` packaging script.

The target package will be installed to a temporary directory and its installed
resources will be collected and packaged.

``package_path`` (string)

   Local filesystem to the directory containing a ``setup.py`` file.

``optimize_level`` (int)

   The module optimization level for packaged bytecode.

   Allowed values are ``0``, ``1``, and ``2``.

   Defaults to ``0``, which is the Python default.

``include_source`` (bool)

   Whether to include the source code for Python modules in addition to the
   bytecode.

   Default is ``true``.

``virtualenv``
^^^^^^^^^^^^^^

This rule will include resources found in a pre-populated *virtualenv*
directory.

.. important::

   PyOxidizer only supports finding modules and resources
   populated via *traditional* means (e.g. ``pip install`` or ``python setup.py
   install``). If ``.pth`` or similar mechanisms are used for installing modules,
   files may not be discovered properly.

``path`` (string)

   The filesystem path to the root of the virtualenv.

   Python modules are typically in a ``lib/pythonX.Y/site-packages`` directory
   under this path.

``optimize_level`` (int)

   The module optimization level for packaged bytecode.

   Allowed values are ``0``, ``1``, and ``2``.

   Defaults to ``0``, which is the Python default.

``excludes`` (array of string)

   An array of package or module names to exclude. See the documentation
   for ``excludes`` for ``package-root`` rules for more.

   Default is an empty array.

``include_source`` (bool)

   Whether to include the source code for modules in addition to the bytecode.

   Default is ``true``.

Example::

   [[python_packages]]
   type = "virtualenv"
   path = "/home/gps/src/myapp/venv"

``filter-include``
^^^^^^^^^^^^^^^^^^

This rule filters all resource names resolved so far through a set of
resource names resolved from sources defined by this section. Resources
not contained in the set defined by this section will be removed.

This rule is effectively an *allow list*. This rule allows earlier rules
to aggressively pull in resources only to filter them via this rule.
This approach is often easier than adding a cherry picked set of resources
via highly granular addition rules.

The section has keys that define various sources for resource names:

``files`` (array of string)

   List of filesystem paths to files containing resource names. The file must
   be valid UTF-8 and consist of a ``\n`` delimited list of resource names.
   Empty lines and lines beginning with ``#`` are ignored.

``glob_files`` (array of string)

   List of glob matching patterns of filter files to read. ``*`` denotes
   all files in a directory. ``**`` denotes recursive directories. This uses
   the Rust ``glob`` crate under the hood and the documentation for that crate
   contains more pattern matching info.

   The files read by this key must be the same format as documented by the
   ``files`` key.

All defined keys have their resolved resources combined into a set of
resource names. Each read entity has its values unioned with the set of
values resolved so far.

Example::

   [[python_packages]]
   type = "filter-include"
   files = ["allow-modules"]
   glob_files = ["module-dumps/modules-*"]

In Combination With ``write_modules_directory_env``
'''''''''''''''''''''''''''''''''''''''''''''''''''

The ``write_modules_directory_env`` Python configuration setting enables
processes to write ``modules-*`` files containing loaded modules to a
directory specified by this environment variable.

This can be combined with the ``files`` and ``glob_files`` keys of the
``filter-include`` rule to build binaries in two phases to *probe* for
loaded modules.

In phase 1, a binary is built with all resources and
``write_modules_directory_env`` enabled. The binary is then executed
and ``modules-*`` files are written.

In phase 2, the file filter is enabled and only the modules used by
the binary will be packaged.

``[[python_run]]``
------------------

This section configures the behavior of the default Python interpreter
and application binary.

The ``PythonConfig`` struct used by the ``pyembed`` crate contains a
default mode of execution for the Python interpreter. The default
Rust application instantiating a ``MainPythonInterpreter`` will execute
this default.

If you are using a custom ``PythonConfig`` or application for
instantiating an interpreter, this setting is not relevant.

Instances of this section have a ``mode`` key that defines what operating
mode the interpreter is in. The sections below describe these
various modes.

``eval``
^^^^^^^^

This mode will evaluate a string containing Python code after the
interpreter initializes.

This mode requires the ``code`` key to be set to a string containing
Python code to run.

Example::

   [[python_run]]
   mode = "eval"
   code = "import mymodule; mymodule.main()"

``module``
^^^^^^^^^^

This mode will load a named Python module as the ``__main__`` module and
then execute that module.

This mode requires the ``module`` key to be set to the string value of
the module to load as ``__main__``.

Example::

   [[python_run]]
   mode = "module"
   module = "mymodule"

``repl``
^^^^^^^^

This mode will launch an interactive Python REPL connected to stdin. This
is similar to the behavior of running a ``python`` executable without any
arguments.

Example::

   [[python_run]]
   mode = "repl"

``noop``
^^^^^^^^

This mode will do nothing. It is provided for completeness sake.
