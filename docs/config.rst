.. _config_files:

===================
Configuration Files
===================

PyOxidizer uses `Starlark <https://github.com/bazelbuild/starlark>`_
files to configure run-time behavior.

Starlark is a dialect of Python intended to be used as a configuration
language and the syntax should be familiar to any Python programmer.

.. _config_finding_configuration_files:

Finding Configuration Files
===========================

If the ``PYOXIDIZER_CONFIG`` environment variable is set, the path specified
by this environment variable will be used as the location of the Starlark
configuration file.

If the ``OUT_DIR`` environment variable is set (we're building from the
context of a Rust project), the ancestor directories will be searched for
a ``pyoxidizer.bzl`` file and the first one found will be used.

Otherwise, ``PyOxidizer`` will look for a ``pyoxidizer.bzl`` file starting in
either the current working directory or from the directory containing the
``pyembed`` crate and then will traverse ancestor directories until a file is
found.

If no configuration file is found, an error occurs.

File Processing Semantics
=========================

A configuration file is evaluated in a custom Starlark *dialect* which
provides primitives used by PyOxidizer. This dialect provides some
well-defined global variables (defined in UPPERCASE) as well as some
types and functions that can be constructed and called. See below
for general usage and :ref:`config_api` for a full reference of what's
available to the Starlark environment.

Since Starlark is effectively a subset of Python, executing a ``PyOxidizer``
configuration file is effectively running a sandboxed Python script. It is
conceptually similar to running ``python setup.py`` to build a Python
package. As functions within the Starlark environment are called,
``PyOxidizer`` will perform actions as described by those functions.

Targets
=======

``PyOxidizer`` configuration files are composed of functions registered
as named *targets*. You define a function that does something then
register it as a target by calling the
:ref:`config_register_target` global function provided by our Starlark
dialect. e.g.:

.. code-block:: python

   def get_python_distribution():
       return default_python_distribution()

   register_target("dist", get_python_distribution)

When a configuration file is evaluated, ``PyOxidizer`` attempts to
*resolve* an ordered list of *targets* This list of targets is either
specified by the end-user or is derived from the configuration file.
The first ``register_target()`` target or the last ``register_target()``
call passing ``default=True`` is the default target.

When evaluated in *Rust build script mode* (typically via
``pyoxidizer run-build-script``), the default target will be the one
specified by the last ``register_target()`` call passing
``default_build_script=True``, or the default target if no target defines
itself as the default build script target.

``PyOxidizer`` calls the registered target functions in order to
*resolve* the requested set of targets.

Target functions can depend on other targets and dependent target functions
will automatically be called and have their return value passed as an
argument to the target function depending on it. See
:ref:`config_register_target` for more.

The value returned by a target function is special. If that value is one
of the special types defined by our Starlark dialect (e.g.
:ref:`PythonDistribution <config_python_distribution>` or
:ref:`PythonExecutable <config_python_executable>`),
``PyOxidizer`` will attempt to invoke special functionality depending
on the run mode. For example, when running ``pyoxidizer build`` to
*build* a target, ``PyOxidizer`` will invoke any *build* functionality
on the value returned by a target function, if present. For example,
a ``PythonExecutable``'s *build* functionality would compile an
executable binary embedding Python.

Common Operations
=================

Obtain a Python Distribution
----------------------------

A :ref:`PythonDistribution <config_python_distribution>` type defines a
Python distribution from which you can derive binaries, perform packaging
actions, etc. Every configuration file will likely utilize this type.

Instances are typically constructed from
:ref:`default_python_distribution() <config_default_python_distribution>`
and are registered as their own target, since multiple targets may want
to reference the distribution instance:

.. code-block:: python

   def make_dist():
      return default_python_distribution()

   register_target("dist", make_dist)

Creating an Executable File Embedding Python
--------------------------------------------

A :ref:`config_python_executable` type defines an executable file embedding
Python.

Instances are derived from a ``PythonDistribution`` instance, usually
by using target dependencies. In this example, we create an executable
that runs a Python REPL on startup:

.. code-block:: python

   def make_dist():
       return default_python_distribution()

   def make_exe(dist):
       return dist.to_python_executable(
           "myapp",
           run_repl=True,
       )

   register_target("dist", make_dist)
   register_target("exe", make_exe, depends=["dist"], default=True)

See :ref:`packaging` for more examples.

Copying Files Next To Your Application
--------------------------------------

The :ref:`FileManifest <config_file_manifest>` type represents a collection of
files and their content. When ``FileManifest`` instances are returned from a
target function, their build action results in their contents being
manifested in a directory having the name of the build target.

``FileManifest`` instances can be used to construct custom file *install
layouts*.

Say you have an existing directory tree of files you want to copy
next to your application.

The :ref:`config_glob` function can be used to discover existing files
on the filesystem and turn them into a ``FileManifest``. You can then
return this ``FileManifest`` directory or overlay it onto another
instance using :ref:`config_file_manifest_add_manifest`. Here's an
example:

.. code-block:: python

   def make_install():
       m = FileManifest()

       templates = glob("/path/to/project/templates/**/*", strip_prefix="/path/to/project/")
       m.add_manifest(templates)

       return m

This will take all files ``/path/to/project/templates/``, strip the path
prefix ``/path/to/project/`` from them and then add all those files to your
main ``FileManifest``. The files should be installed as ``templates/*`` when
the ``InstallManifest`` is materialized.
