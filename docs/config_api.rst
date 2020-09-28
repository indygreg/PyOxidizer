.. _config_api:

================================
Configuration File API Reference
================================

This document describes the low-level API for ``PyOxidizer`` configuration
files. For a higher-level overview of how configuration files work, see
:ref:`config_files`.

.. _config_resource_locations:

Specifying Resource Locations
=============================

Various functionality relates to the concept of a *resource location*, or
where a resource should be loaded from at run-time. See
:ref:`packaging_resources` for more.

Resource locations are represented as strings in Starlark. The mapping
of strings to resource locations is as follows:

``default``
   Use the default resource location. Often equivalent to a resource location
   of the type/value ``None``.

``in-memory``
   Load the resource from memory.

``filesystem-relative:<prefix>``
   Install and load the resource from a filesystem relative path to the
   build binary. e.g. ``filesystem-relative:lib`` will place resources
   in the ``lib/`` directory next to the build binary.

.. _config_python_resources:

Python Resources
================

At run-time, Python interpreters need to consult *resources* like Python
module source and bytecode as well as resource/data files. We refer to all
of these as *Python Resources*.

Configuration files represent *Python Resources* via the types
:ref:`config_type_python_source_module`,
:ref:`config_type_python_package_resource`,
:ref:`config_type_python_package_distribution_resource`,
and :ref:`config_type_python_extension_module`.

These are described in detail in the following sections.

.. _config_python_resources_policy:

Python Resources Policy
=======================

There are various ways to add resources (typically Python resources) to
a binary. For example, you can import modules from memory or the filesystem.
Often, configuration files may wish to be explicit about what behavior is
and is not allowed. A *Python Resources Policy* is used to apply said
behavior.

A *Python Resources Policy* is defined by a ``str``. The following
values are recognized.

``in-memory-only``
   Resources are to be loaded from in-memory only. If a resource cannot be
   loaded from memory (e.g. dynamically linked Python extension modules in
   some configurations), an error will (likely) occur.

``filesystem-relative-only:<prefix>``
   Values starting with ``filesystem-relative-only:`` specify that resources are
   to be loaded from the filesystem from paths relative to the produced
   binary. Files will be installed at the path prefix denoted by the value after
   the ``:``. e.g. ``filesystem-relative-only:lib`` will install resources in a
   ``lib/`` directory.

``prefer-in-memory-fallback-filesystem-relative:<prefix>``
   Values starting with ``prefer-in-memory-fallback-filesystem-relative`` represent
   a hybrid between ``in-memory-only`` and ``filesystem-relative-only:<prefix>``.
   Essentially, if in-memory resource loading is supported, it is used. Otherwise
   we fall back to loading from the filesystem from paths relative to the produced
   binary.

.. _config_python_binaries:

Python Binaries
===============

Binaries containing an embedded Python interpreter can be defined by
configuration files. They are defined via the :ref:`config_type_python_executable`
type. In addition, the :ref:`config_type_python_embedded_resources` type represents
the collection of resources made available to an embedded Python interpreter.

.. _config_type_python_executable:

``PythonExecutable``
--------------------

The ``PythonExecutable`` type represents an executable file containing
the Python interpreter, Python resources to make available to the interpreter,
and a default run-time configuration for that interpreter.

Instances are constructed from ``PythonDistribution`` instances using
:ref:`config_python_distribution_to_python_executable`.

.. _config_type_python_executable_make_python_source_module:

``PythonExecutable.make_python_source_module(name, source, is_package=false)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method creates a ``PythonSourceModule`` instance suitable for use with
the executable being built.

Arguments are as follows:

``name`` (string)
   The name of the Python module. This is the fully qualified module
   name. e.g. ``foo`` or ``foo.bar``.
``source`` (string)
   Python source code comprising the module.
``is_package`` (bool)
   Whether the Python module is also a package. (e.g. the equivalent of a
   ``__init__.py`` file or a module without a ``.`` in its name.

.. _config_type_python_executable_pip_install:

``PythonExecutable.pip_install(args, extra_envs={})``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method runs ``pip install <args>`` with settings appropriate to target
the executable being built.

``args``
   List of strings defining raw process arguments to pass to ``pip install``.

``extra_envs``
   Optional dict of string key-value pairs constituting extra environment
   variables to set in the invoked ``pip`` process.

Returns a ``list`` of objects representing Python resources installed as
part of the operation. The types of these objects can be ``PythonSourceModule``,
``PythonPackageResource``, etc.

The returned resources are typically added to a ``FileManifest`` or
``PythonExecutable`` to make them available to a packaged
application.

.. _config_type_python_executable_read_package_root:

``PythonExecutable.read_package_root(path, packages)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method discovers resources from a directory on the filesystem.

The specified directory will be scanned for resource files. However,
only specific named *packages* will be found. e.g. if the directory
contains sub-directories ``foo/`` and ``bar``, you must explicitly
state that you want the ``foo`` and/or ``bar`` package to be included
so files from these directories will be read.

This rule is frequently used to pull in packages from local source
directories (e.g. directories containing a ``setup.py`` file). This
rule doesn't involve any packaging tools and is a purely driven by
filesystem walking. It is primitive, yet effective.

This rule has the following arguments:

``path`` (string)
   The filesystem path to the directory to scan.

``packages`` (list of string)
   List of package names to include.

   Filesystem walking will find files in a directory ``<path>/<value>/`` or in
   a file ``<path>/<value>.py``.

Returns a ``list`` of objects representing Python resources found in the
virtualenv. The types of these objects can be ``PythonSourceModule``,
``PythonPackageResource``, etc.

The returned resources are typically added to a ``FileManifest`` or
``PythonExecutable`` to make them available to a packaged application.

.. _config_type_python_executable_read_virtualenv:

``PythonExecutable.read_virtualenv(path)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method attempts to read Python resources from an already built
virtualenv.

.. important::

   PyOxidizer only supports finding modules and resources
   populated via *traditional* means (e.g. ``pip install`` or ``python setup.py
   install``). If ``.pth`` or similar mechanisms are used for installing modules,
   files may not be discovered properly.

It accepts the following arguments:

``path`` (string)
   The filesystem path to the root of the virtualenv.

   Python modules are typically in a ``lib/pythonX.Y/site-packages`` directory
   (on UNIX) or ``Lib/site-packages`` directory (on Windows) under this path.

Returns a ``list`` of objects representing Python resources found in the virtualenv.
The types of these objects can be ``PythonSourceModule``,
``PythonPackageResource``, etc.

The returned resources are typically added to a ``FileManifest`` or
``PythonExecutable`` to make them available to a packaged application.

.. _config_type_python_executable_setup_py_install:

``PythonExecutable.setup_py_install(...)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method runs ``python setup.py install`` against a package at the
specified path.

It accepts the following arguments:

``package_path``
   String filesystem path to directory containing a ``setup.py`` to invoke.

``extra_envs={}``
   Optional dict of string key-value pairs constituting extra environment
   variables to set in the invoked ``python`` process.

``extra_global_arguments=[]``
   Optional list of strings of extra command line arguments to pass to
   ``python setup.py``. These will be added before the ``install``
   argument.

Returns a ``list`` of objects representing Python resources installed
as part of the operation. The types of these objects can be
``PythonSourceModule``, ``PythonPackageResource``, etc.

The returned resources are typically added to a ``FileManifest`` or
``PythonExecutable`` to make them available to a packaged application.

.. _config_type_python_executable_add_python_resource:

``PythonExecutable.add_python_resource(...)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method registers a Python resource of various types with the instance.

It accepts a ``resource`` argument which can be a ``PythonSourceModule``,
``PythonPackageResource``, or ``PythonExtensionModule`` and registers that
resource with this instance.

The following arguments are accepted:

``resource``
   The resource to add to the embedded Python environment.

This method is a glorified proxy to the various ``add_python_*`` methods.
Unlike those methods, this one accepts all types that are known Python
resources.

.. _config_type_python_executable_add_python_resources:

``PythonExecutable.add_python_resources(...)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method registers an iterable of Python resources of various types.
This method is identical to
:ref:`config_type_python_executable_add_python_resource` except the argument is
an iterable of resources. All other arguments are identical.

.. _config_type_python_executable_filter_from_files:

``PythonExecutable.filter_from_files(files=[], glob_patterns=[])``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method filters all embedded resources (source modules, bytecode modules,
and resource names) currently present on the instance through a set of
resource names resolved from files.

This method accepts the following arguments:

``files`` (array of string)
   List of filesystem paths to files containing resource names. The file
   must be valid UTF-8 and consist of a ``\n`` delimited list of resource
   names. Empty lines and lines beginning with ``#`` are ignored.

``glob_files`` (array of string)
   List of glob matching patterns of filter files to read. ``*`` denotes
   all files in a directory. ``**`` denotes recursive directories. This
   uses the Rust ``glob`` crate under the hood and the documentation for that
   crate contains more pattern matching info.

   The files read by this argument must be the same format as documented
   by the ``files`` argument.

All defined files are first read and the resource names encountered are
unioned into a set. This set is then used to filter entities currently
registered with the instance.

.. _config_type_python_executable_to_embedded_resources:

``PythonExecutable.to_embedded_resources()``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Obtains a :ref:`config_type_python_embedded_resources` instance representing
resources to be made available to the Python interpreter.

See the :ref:`config_type_python_embedded_resources` type documentation for more.

Interacting With the Filesystem
===============================

.. _config_type_file_manifest:

``FileManifest()``
------------------

The ``FileManifest`` type represents a set of files and their content.

``FileManifest`` instances are used to represent things like the final
filesystem layout of an installed application.

Conceptually, a ``FileManifest`` is a dict mapping relative paths to
file content.

.. _config_type_file_manifest_add_manifest:

``FileManifest.add_manifest(manifest)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method overlays another ``FileManifest`` on this one. If the other
manifest provides a path already in this manifest, its content will be
replaced by what is in the other manifest.

``FileManifest.add_python_resource(prefix, value)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method adds a Python resource to a ``FileManifest`` instance in
a specified directory prefix. A *Python resource* here can be a
``PythonSourceModule``, ``PythonPackageResource``,
``PythonPackageDistributionResource``,  or ``PythonExtensionModule``.

This method can be used to place the Python resources derived from another
type or action in the filesystem next to an application binary.

``FileManifest.add_python_resources(prefix, values)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method adds an iterable of Python resources to a ``FileManifest``
instance in a specified directory prefix. This is effectively a wrapper
for ``for value in values: self.add_python_resource(prefix, value)``.

For example, to place the Python distribution's standard library Python
source modules in a directory named ``lib``::

   m = FileManifest()
   dist = default_python_distribution()
   m.add_python_resources(dist.source_modules())

``FileManifest.install(path, replace=True)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method writes the content of the ``FileManifest`` to a directory
specified by ``path``. The path is evaluated relative to the path
specified by ``BUILD_PATH``.

If ``replace`` is True (the default), the destination directory will
be deleted and the final state of the destination directory should
exactly match the state of the ``FileManifest``.

.. _config_type_file_content:

``FileContent``
---------------

This type represents the content of a single file.

.. _config_glob:

``glob(include, exclude=None, strip_prefix=None)``
--------------------------------------------------

The ``glob()`` function resolves file patterns to a ``FileManifest``.

``include`` is a ``list`` of ``str`` containing file patterns that will be
matched using the ``glob`` Rust crate. If patterns begin with ``/`` or
look like a filesystem absolute path, they are absolute. Otherwise they are
evaluated relative to the directory of the current config file.

``exclude`` is an optional ``list`` of ``str`` and is used to exclude files
from the result. All patterns in ``include`` are evaluated before ``exclude``.

``strip_prefix`` is an optional ``str`` to strip from the beginning of
matched files. ``strip_prefix`` is stripped after ``include`` and ``exclude``
are processed.

Returns a ``FileManifest``.
