.. _config_type_python_source_module:

======================
``PythonSourceModule``
======================

This type represents Python source modules, agnostic of location.

Each instance has the following attributes:

``name`` (``string``)
   Fully qualified name of the module. e.g. ``foo.bar``.

``source`` (``string``)
   The Python source code for this module.

``is_package`` (``bool``)
   Whether this module is also a Python package (or sub-package).

``is_stdlib`` (``bool``)
   Whether this module is part of the Python standard library (part of the
   Python distribution).

``add_include`` (``bool``) (mutable)
   Whether to actually add this resource when it is added to a binary.

   If set to ``false``, requests to add the resource will result in no-ops.

``add_location`` (``string``) (mutable)
   Defines the location from which this resource should be loaded when added
   to a binary.

``add_location_fallback`` (``string`` or ``None``) (mutable)
   Defines a fallback location from which this resource should be loaded when
   added to a binary. Only used if attempts to add to ``add_location`` fail.

   Can be set to ``None`` to disable fallback.

``add_source`` (``bool``)
   Whether to add source code for this module.

   If set to false, only bytecode will possibly be added.

``add_bytecode_optimization_level_zero``
   Whether to generate and add bytecode at optimization level 0.

``add_bytecode_optimization_level_one``
   Whether to generate and add bytecode at optimization level 1.

``add_bytecode_optimization_level_two``
   Whether to generate and add bytecode at optimization level 2.

Instances can be constructed via
:ref:`config_python_executable_make_python_source_module`.
