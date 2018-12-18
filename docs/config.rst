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

``[python_packaging]``
======================

Configures how the Python interpreter is packaged. Options in this section
declare what Python packages/modules are included in the embedded Python
distribution.

optimize_level
   The module optimization level for packaged bytecode.

   Allowed values are ``0``, ``1``, and ``2``.

   Defaults to ``0``, which is the Python default.

module_paths
   Array of filesystem paths containing extra Python modules to package and
   make available for import. Any ``.py`` files under these directories will
   be compiled to bytecode and made available for import.

   If a relative path exists in multiple directories, the first encountered
   path is used.

   To easily package all dependencies required for an application, one can
   create a virtualenv and point this config option at its e.g.
   ``lib/python3.7/site-packages`` directory.

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