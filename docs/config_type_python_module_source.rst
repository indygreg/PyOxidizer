.. _config_type_python_module_source:

======================
``PythonModuleSource``
======================

This type represents Python source modules, agnostic of location.

Instances can be constructed via
:ref:`config_python_executable_make_python_module_source` or by calling
methods that emit Python resources.

Attributes
==========

The following sections describe the attributes available on each
instance.

.. _config_type_python_source_module_name:

``name``
--------

(``string``)

Fully qualified name of the module. e.g. ``foo.bar``.

.. _config_type_python_source_module_source:

``source``
----------

(``string``)

The Python source code for this module.

.. _config_type_python_source_module_is_package:

``is_package``
--------------

(``bool``)

Whether this module is also a Python package (or sub-package).

.. _config_type_python_source_module_is_stdlib:

``is_stdlib``
-------------

(``bool``)

Whether this module is part of the Python standard library (part of the
Python distribution).

``add_*``
---------

(various)

See :ref:`config_resource_add_attributes`.