.. _config_type_python_package_resource:

=========================
``PythonPackageResource``
=========================

This type represents a resource _file_ in a Python package. It is
effectively a named blob associated with a Python package. It is
typically accessed using the ``importlib.resources`` API.

Attributes
==========

The following sections describe the attributes available on each
instance.

.. _config_type_python_package_resource_package:

``package``
-----------

(``string``)

Python package this resource is associated with.

.. _config_type_python_package_resource_name:

``name``
--------

(``string``)

Name of this resource.

.. _config_type_python_package_resource_is_stdlib:

``is_stdlib``
-------------

(``bool``)

Whether this module is part of the Python standard library (part of the
Python distribution).

``add_*``
---------

(various)

See :ref:`config_resource_add_attributes`.
