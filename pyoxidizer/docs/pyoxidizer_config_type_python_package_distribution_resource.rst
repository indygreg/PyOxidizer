.. _config_type_python_package_distribution_resource:

=====================================
``PythonPackageDistributionResource``
=====================================

This type represents a named resource to make available as Python package
distribution metadata. These files are typically accessed using the
``importlib.metadata`` API.

Each instance represents a logical file in a ``<package>-<version>.dist-info``
or ``<package>-<version>.egg-info`` directory. There are specifically named
files that contain certain data. For example, a ``*.dist-info/METADATA`` file
describes high-level metadata about a Python package.

Attributes
==========

The following sections describe the attributes available on each
instance.

.. _config_type_python_package_distribution_resource_package:

``package``
-----------

(``string``)

Python package this resource is associated with.

.. _config_type_python_package_distribution_resource_name:

``name``
--------

(``string``)

Name of this resource.

.. _config_type_python_package_distribution_resource_is_stdlib:

``is_stdlib``
-------------

(``bool``)

Whether this module is part of the Python standard library (part of the
Python distribution).

``add_*``
---------

(various)

See :ref:`config_resource_add_attributes`.
