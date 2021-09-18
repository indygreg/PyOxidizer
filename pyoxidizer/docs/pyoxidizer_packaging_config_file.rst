.. py:currentmodule:: starlark_pyoxidizer

.. _packaging_config_file:

================================================
Packaging Primitives in ``pyoxidizer.bzl`` Files
================================================

PyOxidizer's run-time behavior is controlled by ``pyoxidizer.bzl``
Starlark (a Python-like language) configuration files. See :ref:`config_files`
for documentation on these files, including low-level API documentation.

This document gives a medium-level overview of the important Starlark
types and functions and how they all interact.

Targets Define Actions
======================

As detailed at :ref:`config_processing_targets`, a PyOxidizer configuration
file is composed of named *targets*, which are functions returning an object
that may have a build or run action attached. Commands like
``pyoxidizer build`` identify a target to evaluate then effectively
walk the dependency graph evaluating dependent targets until the
requested target is *built*.

.. _packaging_config_type_python_executable:

Defining an Executable Embedding Python
=======================================

In this example, we create an executable embedding Python:

.. code-block:: python

    def make_exe():
        dist = default_python_distribution()

        return dist.to_python_executable("myapp")

    register_target("exe", make_exe)
    resolve_targets()

:py:meth:`PythonDistribution.to_python_executable` accepts an optional
:py:class:`PythonPackagingPolicy` instance that influences how the executable
is built and what resources are added where. See the
:py:class:`type documentation <PythonPackagingPolicy>` for the
list of parameters that can be influenced. Some of this behavior
is described in the sections below. Other examples are provided
throughout the :ref:`packaging` documentation.

.. _packaging_config_interpreter_config:

Configuring the Python Interpreter Run-Time Behavior
====================================================

The :py:class:`PythonInterpreterConfig`
Starlark type configures the default behavior of the Python interpreter
embedded in built binaries.

A :py:class:`PythonInterpreterConfig` instance is associated with
:py:class:`PythonExecutable` instances when they are created. A custom
instance can be passed into :py:meth:`PythonDistribution.to_python_executable`
to use non-default settings.

In this example (similar to above), we construct a custom
:py:class:`PythonInterpreterConfig` instance using non-defaults and then pass
this instance into the constructed :py:class:`PythonExecutable`:

.. code-block:: python

    def make_exe():
        dist = default_python_distribution()

        config = dist.make_python_interpreter_config()
        config.run_command = "print('hello, world')"

        return dist.to_python_executable("myapp", config=config)

    register_target("exe", make_exe)
    resolve_targets()

The :py:class:`PythonInterpreterConfig` type exposes a lot of modifiable settings.
See the :py:class:`API documentation <PythonInterpreterConfig>` for
the complete list. These settings include but are not limited to:

* Control of low-level Python interpreter settings, such as whether
  environment variables (like ``PYTHONPATH``) should influence run-time
  behavior, whether stdio should be buffered, and the filesystem encoding
  to use.
* Whether to enable the importing of Python modules from the filesystem
  and what the initial value of ``sys.path`` should be.
* The memory allocator that the Python interpreter should use.
* What Python code to run when the interpreter is started.
* How the ``terminfo`` database should be located.

Many of these settings are not needed for most programs and the defaults
are meant to be reasonable for most programs. However, some settings - such
as the ``run_*`` arguments defining what Python code to run by default - are
required by most configuration files.

.. _packaging_config_python_packages:

Adding Python Packages to Executables
=====================================

A just-created :py:class:`PythonExecutable` Starlark type contains just the
Python interpreter and standard library derived from the :py:class:`PythonDistribution`
from which it came. While you can use PyOxidizer to produce an executable
containing just a normal Python *distribution* with nothing else, many people
will want to add their own Python packages/code.

The Starlark environment defines various types for representing Python
package resources. These include
:py:class:`PythonModuleSource`, :py:class:`PythonExtensionModule`,
:py:class:`PythonPackageDistributionResource`, and more.

Instances of these types can be created dynamically or by performing
common Python packaging operations (such as invoking ``pip install``) via
various methods on :py:class:`PythonExecutable` instances. These Python package
resource instances can then be added to :py:class:`PythonExecutable` instances
so they are part of the built binary.

See :ref:`packaging_resources` and :ref:`packaging_python_files`
for more on this topic, including many examples.

.. _packaging_config_install_manifests:

Install Manifests Copy Files Next to Your Application
=====================================================

The :py:class:`starlark_tugger.FileManifest` Starlark type represents a
collection of files and their content. When
:py:class:`starlark_tugger.FileManifest` instances are returned from a
target function, their build action results in their contents
being manifested in a directory having the name of the build target.

:py:class:`starlark_tugger.FileManifest` instances can be used to
construct custom file *install layouts*.

Say you have an existing directory tree of files you want to copy
next to your built executable defined by the :py:class:`PythonExecutable` type.

The :py:func:`starlark_tugger.glob` function can be used to discover
existing files on the filesystem and turn them into a
:py:class:`starlark_tugger.FileManifest`. You can then return this
:py:class:`starlark_tugger.FileManifest` directory or overlay it onto another
instance using
:py:meth:`starlark_tugger.FileManifest.add_manifest`. Here's an example:

.. code-block:: python

    def make_exe():
        dist = default_python_distribution()

        return dist.to_python_executable("myapp")

    def make_install(exe):
        m = FileManifest()

        m.add_python_resource(".", exe)

        templates = glob(["/path/to/project/templates/**/*"], strip_prefix="/path/to/project/")
        m.add_manifest(templates)

        return m

    register_target("exe", make_exe)
    register_target("install", make_install, depends=["exe"], default=True)
    resolve_targets()

We introduce a new ``install`` target and ``make_install()`` function which
returns a :py:class:`starlark_tugger.FileManifest`. It adds the
:py:class:`PythonExecutable` (represented by the ``exe`` argument/variable) to
that manifest in the root directory, signified by ``.``.

Next, it calls ``glob()`` to find all files in the
``/path/to/project/templates/`` directory tree, strips the path prefix
``/path/to/project/`` from them, and then merges all of these files into
the final manifest.

When the ``InstallManifest`` is built, the final layout should look something
like the following:

* ``install/myapp`` (or ``install/myapp.exe`` on Windows)
* ``install/templates/foo``
* ``install/templates/...``

See :ref:`packaging_additional_files` for more on this topic.
