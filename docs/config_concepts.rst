.. _config_concepts:

========
Concepts
========

.. _config_processing:

Processing
==========

A configuration file is evaluated in a custom Starlark *dialect* which
provides primitives used by PyOxidizer. This dialect provides some
well-defined global variables (defined in UPPERCASE) as well as some
types and functions that can be constructed and called. See
:ref:`config_globals` for a full list of what's available to the
Starlark environment.

Since Starlark is effectively a subset of Python, executing a ``PyOxidizer``
configuration file is effectively running a sandboxed Python script. It is
conceptually similar to running ``python setup.py`` to build a Python
package. As functions within the Starlark environment are called,
``PyOxidizer`` will perform actions as described by those functions.

.. _config_processing_targets:

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
:ref:`config_type_python_distribution` or
:ref:`config_type_python_executable`),
``PyOxidizer`` will attempt to invoke special functionality depending
on the run mode. For example, when running ``pyoxidizer build`` to
*build* a target, ``PyOxidizer`` will invoke any *build* functionality
on the value returned by a target function, if present. For example,
a ``PythonExecutable``'s *build* functionality would compile an
executable binary embedding Python.

.. _config_concept_python_distribution:

Python Distributions Provide Python
===================================

The :ref:`config_type_python_distribution` Starlark
type defines a Python distribution. A Python distribution is an entity
which contains a Python interpreter, Python standard library, and which
PyOxidizer knows how to consume and integrate into a new binary.

``PythonDistribution`` instances are arguably the most important type
in configuration files because without them you can't perform Python
packaging actions or construct binaries with Python embedded.

Instances of ``PythonDistribution`` are typically constructed from
:ref:`default_python_distribution() <config_default_python_distribution>`
and are registered as their own target, since multiple targets may want
to reference the distribution instance:

.. code-block:: python

   def make_dist():
      return default_python_distribution()

   register_target("dist", make_dist)

.. _config_concept_python_executable:

Python Executables Run Python
=============================

The :ref:`config_type_python_executable` Starlark type
defines an executable file embedding Python. Instances of this type
are used to build an executable file (and possibly other files needed
by it) that contains an embedded Python interpreter and other resources
required by it.

Instances of ``PythonExecutable`` are derived from a ``PythonDistribution``
instance via the
:ref:`PythonDistribution.to_python_executable() <config_python_distribution_to_python_executable>`
method. There is typically a standalone function/target in config files
for doing this.

.. _config_python_resources:

Python Resources
================

At run-time, Python interpreters need to consult *resources* like Python
module source and bytecode as well as resource/data files. We refer to all
of these as *Python Resources*.

Configuration files represent *Python Resources* via the following types:

* :ref:`config_type_python_source_module`
* :ref:`config_type_python_package_resource`
* :ref:`config_type_python_package_distribution_resource`
* :ref:`config_type_python_extension_module`

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

.. _config_python_resources_policy:

Python Resources Policy
=======================

There are various ways to add resources (typically Python resources) to
a binary. For example, you can import modules from memory or the filesystem.
Often, configuration files may wish to be explicit about what behavior is
and is not allowed. A *Python Resources Policy* is used to apply said
behavior.

A *Python Resources Policy* is defined by a ``string``. The following
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
