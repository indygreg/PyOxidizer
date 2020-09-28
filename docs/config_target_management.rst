.. _config_target_management:

==============================
Functions for Managing Targets
==============================

.. _config_register_target:

``register_target()``
=====================

Registers a named target that can be resolved by the configuration file.

A target consists of a string name, callable function, and an optional list
of targets it depends on.

The callable may return one of the types defined by this Starlark dialect
to facilitate additional behavior, such as how to build and run it.

Arguments:

``name``
   (``string``) The name of the target being register.

``fn``
   (``function``) A function to call when the target is resolved.

``depends``
   (``list`` of ``string`` or ``None``) List of target strings this target
   depends on. If specified, each dependency will be evaluated in order and
   its returned value (possibly cached from prior evaluation) will be passed
   as a positional argument to this target's callable.

``default``
   (``bool``) Indicates whether this should be the default target
   to evaluate. The last registered target setting this to ``True``
   will be the default. If no target sets this to ``True``, the first
   registered target is the default.

``default_build_script``
   (``bool``) indicates whether this should be the default target to
   evaluate when run from the context of a Rust build script (e.g. from
   ``pyoxidizer run-build-script``. It has the same semantics as
   ``default``.

.. note::

   It would be easier for target functions to call ``resolve_target()``
   within their implementation. However, Starlark doesn't allow recursive
   function calls. So invocation of target callables must be handled
   specially to avoid this recursion.

.. _config_resolve_target:

``resolve_target()``
====================

Triggers resolution of a requested build target.

This function resolves a target registered with ``register_target()`` by
calling the target's registered function or returning the previously
resolved value from calling it.

This function should be used in cases where 1 target depends on the
resolved value of another target. For example, a target to create a
``FileManifest`` may wish to add a ``PythonExecutable`` that was resolved
from another target.

.. _config_resolve_targets:

``resolve_targets()``
=====================

Triggers resolution of requested build targets.

This is usually the last meaningful line in a config file. It triggers the
building of targets which have been requested to resolve by whatever is invoking
the config file.