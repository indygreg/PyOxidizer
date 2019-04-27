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

``[[python_packages]]``
=======================

Configures the packaging of Python packages/modules.

Each entry of this section describes a specific source/rule for finding
Python packages/modules to include. Each entry has a ``type`` field describing
the type of source. All other fields are dependent on the type.

Each section is processed in order and is resolved to a set of named Python
modules/resources. If multiple sections provide the same module/resource, the
last encountered instance of a named entity is used.

The following sections describe the various ``type``s of sources.

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
