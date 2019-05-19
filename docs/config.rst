===================
Configuration Files
===================

The ``pyembed`` crate is configured using a TOML file. This document describes
the format of those TOML files.

Sections in this document correspond to TOML sections.

``[python_distribution]``
=========================

Configures the Python distribution that should be ingested and used.

A Python distribution is a zstandard-compressed tar archive containing a
specially produced Python distribution. These distributions are typically
produced by the
`python-build-standalone <https://github.com/indygreg/python-build-standalone>`_
project. Pre-built distributions are available at
https://github.com/indygreg/python-build-standalone/releases.

A distribution is defined by a location and a hash. The location is
defined by one of the following keys in this section:

local_path
   Local filesystem path containing zstandard-compressed tar archive.

url
   URL from which a zstandard-compressed tar archive can be retrieved using
   an HTTP GET request.

The hash of the archive MUST be defined in the ``sha256`` key.

e.g.::

    [python_distribution]
    local_path = "/var/python-distributions/cpython-linux64.tar.zst"
    sha256 = "11a53f5755773f91111a04f6070a6bc00518a0e8e64d90f58584abf02ca79081"

``[python_config]``
===================

Configures the default configuration settings for the embedded Python
interpreter.

Embedded Python interpreters are configured and instantiated using a
``pyembed::PythonConfig`` data structure. The ``pyembed`` crate defines a
default instance of this data structure with parameters defined by the settings
in this TOML section.

If you are constructing a custom ``pyembed::PythonConfig`` instance and don't
use the default instance, this config section is not relevant to you.

The following keys can be defined to control the default behavior:

dont_write_bytecode
   Controls the value of
   `Py_DontWriteBytecodeFlag <https://docs.python.org/3/c-api/init.html#c.Py_DontWriteBytecodeFlag>`_.
   Default is ``true``.

ignore_environment
   Controls the value of
   `Py_IgnoreEnvironmentFlag <https://docs.python.org/3/c-api/init.html#c.Py_IgnoreEnvironmentFlag>`_.
   Default is ``true``.

no_site
   Controls the value of
   `Py_NoSiteFlag Py <https://docs.python.org/3/c-api/init.html#c.Py_NoSiteFlag>`_.
   Default is ``true``.

no_user_site_directory
   Controls the value of
   `Py_NoUserSiteDirectory <https://docs.python.org/3/c-api/init.html#c.Py_NoUserSiteDirectory>`_.
   Default is ``true``.

optimize_level
   Controls the value of
   `Py_OptimizeFlag <https://docs.python.org/3/c-api/init.html#c.Py_OptimizeFlag>`_.
   Default is ``0``, which is the Python default. Only the values ``0``, ``1``, and
   ``2`` are accepted.

program_name
   The name of the running application. If defined, this value will be passed
   to ``Py_SetProgramName()``.

stdio_encoding
   Defines the encoding and error handling mode for Python's standard I/O
   streams. Values are of the form ``encoding:error``. e.g. ``utf-8:ignore``
   or ``latin-1:strict``. If defined, the ``Py_SetStandardStreamEncoding()``
   function is called during Python interpreter initialization.

unbuffered_stdio
   Controls the value of
   `Py_UnbufferedStdioFlag <https://docs.python.org/3/c-api/init.html#c.Py_UnbufferedStdioFlag>`_.
   Default is ``false``.

filesystem_importer
   Controls whether to enable Python's filesystem based importer. Enabling
   this importer allows Python modules to be imported from the filesystem.

   Default is ``false`` (since we prefer to import modules from memory).

sys_paths
   Defines filesystem paths to be added to ``sys.path``.

   Value is an array of string.

   The special token ``$ORIGIN`` in values will be expanded to the absolute
   path of the executable at run-time.

   Setting this value will imply ``filesystem_importer = true``.

   Default is an empty array.

rust_allocator_raw
   Whether to use the Rust memory allocator for the ``PYMEM_DOMAIN_RAW``
   allocator.

   When set, Python uses Rust's global memory allocator instead of
   ``malloc()``, ``free()``, etc.

   If a custom Rust memory allocator (such as jemalloc as provided by the
   ``jemallocator`` crate) is used, Python will also use this allocator.

   Default is ``true``.

write_modules_directory_env
   Environment variable that defines a directory where ``modules-<UUID>`` files
   containing a ``\n`` delimited list of loaded Python modules (from ``sys.modules``)
   will be written upon interpreter shutdown.

   If this setting is not defined or if the environment variable specified by its
   value is not present at run-time, no special behavior will occur. Otherwise,
   the environment variable's value is interpreted as a directory, that directory
   and any of its parents will be created, and a ``modules-<UUID>`` file will
   be written to the directory.

   This setting is useful when combined with the ``filter-file-include`` packaging
   rule to assemble a list of modules required by a binary. One can use this
   setting to produce a *probing* executable, run that executable (say by
   executing a test harness), then combine the generated files into a unified
   list of modules and use with ``filter-file-include``.

``[[python_packages]]``
=======================

Configures the packaging of Python packages/modules/extensions.

Each entry of this section describes a specific source/rule for finding
Python packages/modules/extensions to include. Each entry has a ``type`` field
describing the type of source. All other fields are dependent on the type.

Each section is processed in order and is resolved to a set of named Python
modules/resources/extensions. If multiple sections provide the same
module/resource/extension, the last encountered instance of a named entity is
used. Essentially, we start with an empty dictionary and update the
dictionary as rules are processed.

Packaging resources are differentiated by type:

* Extension modules
* Python module source
* Python module bytecode
* Resource file

An *extension module* is a Python module backed by compiled code (typically
written in C). Extension modules can have library dependencies. If an extension
module has a library dependency, that library will automatically be linked
with the resulting binary, preferably statically. For example, the
``_sqlite3`` extension module will link the ``libsqlite3`` library (which should
be included as part of the Python distribution).

*Python module source* and *Python module bytecode* refer to ``.py`` and
``.pyc`` files. A bytecode file is derived from a ``.py`` file by compiling
it.

The following sections describe the various ``type``s of sources/rules.

``stdlib-extensions-policy``
----------------------------

``type = "stdlib-extensions-policy"`` defines a base policy for what
extension modules from the Python distribution to include.

This type has a ``policy`` key denoting the extension module policy.
This key can have the following values::

``minimal``
   Include a minimal set of extension modules. Only the extension modules
   required to initialize a Python interpreter will be included.

   This is the default behavior.

``all``
   Include all available extension modules.

``no-libraries``
   Include all extension modules that do not have additional library
   dependencies. Most common Python extension modules are includes. Extension
   modules like ``_ssl`` (links against OpenSSL) and ``zlib`` are not
   included.

``stdlib-extensions-explicit-includes``
---------------------------------------

``type = "stdlib-extensions-explicit-includes`` will include extension
modules from the distribution's standard library if the extension name
is included in a list specified by the ``includes`` key.

This can be combined with the ``minimal`` extension modules policy to
supplement the extension modules that are included.

Example usage::

   [[python_packages]]
   type = "stdlib-extensions-explicit-includes"
   includes = ["binascii", "errno", "itertools", "math", "select", "_socket"]

``stdlib-extensions-explicit-excludes``
---------------------------------------

``type = "stdlib-extensions-explicit-excludes"`` will exclude extension
modules from the distribution's standard library if the extension name
is included in a list specified by the ``excludes`` key.

Example usage::

   [[python_packages]]
   type = "stdlib-extensions-explicit-excludes"
   excludes = ["_ssl"]

``stdlib-extension-variant``
----------------------------

``type = "stdlib-extension-variant"`` denotes to include a specific extension
module variant from the Python distribution.

Some distributions offer multiple options for individual extension modules.
For example, the ``readline`` extension module may offer a ``libedit``
variant that is compiled against ``libedit`` instead of ``libreadline``.

By default, the first listed variant in a Python distribution is used. By
defining entries of this type, alternate extension implementations can be
used.

Extension variants are defined by an extension name and variant name, which
are defined by the ``extension`` and ``variant`` keys, respectively.

Example usage::

   [[python_packages]]
   type = "stdlib-extension-variant"
   extension = "readline"
   variant = "libedit"

``stdlib``
----------

``type = "stdlib"`` denotes Python modules coming from the Python
distribution's standard library.

.. important::

   A ``stdlib`` entry is required, as Python can't be initialized without
   some modules from the standard library. It should almost always be the
   first ``[[python_packages]]`` entry in the config file.

The following keys control behavior:

exclude_test_modules

   A boolean indicating whether test-only modules should be excluded from
   packaging. The Python standard library typically ships various packages
   and modules used for testing Python itself.

   These modules are not referenced by *real* modules in the Python standard
   library and are excluded by default. Support for including them is provided
   for completeness sake, in case someone may want to run the Python standard
   library unit tests with PyOxidizer.

optimize_level
   The module optimization level for packaged bytecode.

   Allowed values are ``0``, ``1``, and ``2``.

   Defaults to ``0``, which is the Python default.

include_source
   Whether to include the source code for modules in addition to the bytecode.
   Defaults to true.

``package-root``
----------------

``type = "package-root"`` denotes packaging of modules and resources from
a directory on the filesystem.

The specified directory will be scanned for Python module and resource files.
However, only specific named *packages* will be packaged. e.g. if the
directory contains directories ``foo/`` and ``bar/``, you must explicitly
state that you want the ``foo`` and/or ``bar`` package to be included so
files from these directories are included.

This type is frequently used to pull in packages from local source
directories (e.g. directories containing a ``setup.py`` file).

The following keys control behavior:

path
   The filesystem path to the directory to scan.

optimize_level
   The module optimization level for packaged bytecode.

   Allowed values are ``0``, ``1``, and ``2``.

   Defaults to ``0``, which is the Python default.

packages
   An array of package names to include. This corresponds to
   ``<package>.py`` files in the root directory or directories of the
   entry's name.

excludes
   An array of package or module names to exclude. By default this is an
   empty array.

   A value in this array will match on an exact full module name match or on
   a package prefix match. e.g. ``foo`` will match the module ``foo``, the
   package ``foo``, and any sub-modules in ``foo``, e.g. ``foo.bar``. But
   it will not match ``foofoo``.

include_source
   Whether to include the source code for modules in addition to the bytecode.
   Defaults to true.

``virtualenv``
--------------

``type = "virtualenv"`` denotes packaging of modules and resources in a
populated virtualenv.

.. important::

   PyOxidizer only supports finding modules and resources populated via
   *traditional* means (e.g. ``pip install`` or ``python setup.py install``).
   If ``.pth`` or similar alternative mechanisms for installing modules are
   used, files may not be discovered properly.

The following keys control behavior:

path
   The filesystem path to the root of the virtualenv.

   Python modules are typically in a ``lib/pythonX.Y/site-packages`` directory
   under this path.

optimize_level
   The module optimization level for packaged bytecode.

   Allowed values are ``0``, ``1``, and ``2``.

   Defaults to ``0``, which is the Python default.

excludes
   An array of package or module names to exclude. By default this is an empty
   array.

   See the documentation for ``excludes`` in ``package-root`` for more.

include_source
   Whether to include the source code for modules in addition to the bytecode.
   Defaults to true.

``pip-install-simple``
----------------------

``type = "pip-install-simple"`` will run ``pip install`` for a single named
package string and will automatically package all the Python resources
associated with that package (and its dependencies).

The following keys control behavior:

package
   Name of the package to install. This is added as a positional argument to
   ``pip install``.

optimize_level
   The module optimization level for packaged bytecode.

   Allowed values are ``0``, ``1``, and ``2``.

   Defaults to ``0``, which is the Python default.

include_source
   Whether to include the source code for Python modules in addition to
   the bytecode. Defaults to true.

Example usage::

   [[python_packages]]
   type = "pip-install-simple"
   package = "pyflakes"

``filter-file-include``
-----------------------

``type = "filter-file"`` will filter all resources captured so far through a
list of resource names read from a file. If a resource captured so far exists
in the file, it will be packaged. Otherwise it will be excluded.

Resource names match module names, resource file names, and extension names.

This rule allows earlier rules to aggressively pull in resources then exclude
resources via omission. This is often easier than cherry picking exactly
which resources to include in highly-granular rules.

The following keys control behavior:

``path``
   The filesystem path of the file containing resource names. The file must
   be valid UTF-8 and consist of a ``\n`` delimited list of resource names.
   Empty lines and lines beginning with ``#`` are ignored.

``[python_run]``
================

Configures the behavior of the default Python interpreter and application
binary.

The ``pyembed`` crate contains a default configuration for running a Python
interpreter and the ``pyapp`` application uses it. This section controls what
Python code is run when the interpreter starts.

The ``mode`` key defines what operation mode the interpreter/application
is in. The sections below describe the various modes.

``eval``
--------

``mode = "eval"`` will evaluate a string of Python code when the interpreter
starts.

This mode requires the ``code`` key to be set to a string containing Python
code to run. e.g.::

   [python_run]
   mode = "eval"
   code = "import mymodule; mymodule.main()"

``module``
----------

``mode = "module"`` will load a named module as the ``__main__`` module and
then execute it.

This mode requires the ``module`` key to be set to the string value of the
module to load as ``__main__``. e.g.::

   [python_run]
   mode = "module"
   module = "mymodule"

``repl``
--------

``mode = "repl"`` will launch an interactive Python REPL console connected to
stdin. This is similar to the behavior of running a ``python`` executable
without any arguments.
