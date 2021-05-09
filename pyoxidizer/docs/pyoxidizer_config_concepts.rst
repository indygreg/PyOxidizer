.. py:currentmodule:: starlark_pyoxidizer

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

The value returned by a target function is special. Some types defined by
our Starlark dialect have special *build* or *run* behavior associated
with them. If you run ``pyoxidizer build`` or ``pyoxidizer run`` against
a target that returns one of these types, that behavior will be performed.

For example, if you return a :py:class:`PythonExecutable`, the
*build* behavior is to produce that executable file and the *run* behavior
is to run that built executable.

See :ref:`config_types_with_target_behavior` for the full list of types
with registered target behaviors.

.. _config_concept_python_distribution:

Python Distributions Provide Python
===================================

The :py:class:`PythonDistribution` Starlark type defines a Python distribution.
A Python distribution is an entity which contains a Python interpreter, Python
standard library, and which PyOxidizer knows how to consume and integrate into
a new binary.

:py:class:`PythonDistribution` instances are arguably the most important type
in configuration files because without them you can't perform Python
packaging actions or construct binaries with Python embedded.

Instances of :py:class:`PythonDistribution` are typically constructed from
:py:func:`default_python_distribution`.

.. _config_concept_python_executable:

Python Executables Run Python
=============================

The :py:class:`PythonExecutable` Starlark type
defines an executable file embedding Python. Instances of this type
are used to build an executable file (and possibly other files needed
by it) that contains an embedded Python interpreter and other resources
required by it.

Instances of :py:class:`PythonExecutable` are derived from a
:py:class:`PythonDistribution` instance via
:py:meth:`PythonDistribution.to_python_executable`. There is typically a
standalone function/target in config files for doing this.

.. _config_python_resources:

Python Resources
================

At run-time, Python interpreters need to consult *resources* like Python
module source and bytecode as well as resource/data files. We refer to all
of these as *Python Resources*.

Configuration files represent *Python Resources* via the following types:

* :py:class:`PythonModuleSource`
* :py:class:`PythonPackageResource`
* :py:class:`PythonPackageDistributionResource`
* :py:class:`PythonExtensionModule`

.. _config_resource_locations:

Specifying Resource Locations
=============================

Various functionality relates to the concept of a *resource location*, or
where a resource should be loaded from at run-time. See
:ref:`packaging_resources` for more.

Resource locations are represented as strings in Starlark. The mapping
of strings to resource locations is as follows:

``in-memory``
   Load the resource from memory.

``filesystem-relative:<prefix>``
   Install and load the resource from a filesystem relative path to the
   build binary. e.g. ``filesystem-relative:lib`` will place resources
   in the ``lib/`` directory next to the build binary.
