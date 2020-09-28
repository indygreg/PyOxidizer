.. _config_processing:

=============================
Configuration File Processing
=============================

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
:ref:`PythonDistribution <config_type_python_distribution>` or
:ref:`PythonExecutable <config_python_executable>`),
``PyOxidizer`` will attempt to invoke special functionality depending
on the run mode. For example, when running ``pyoxidizer build`` to
*build* a target, ``PyOxidizer`` will invoke any *build* functionality
on the value returned by a target function, if present. For example,
a ``PythonExecutable``'s *build* functionality would compile an
executable binary embedding Python.
