.. py:currentmodule:: starlark_pyoxidizer

=====================================
``PythonPackageDistributionResource``
=====================================

.. py:class:: PythonPackageDistributionResource

    This type represents a named resource to make available as Python package
    distribution metadata. These files are typically accessed using the
    ``importlib.metadata`` API.

    Each instance represents a logical file in a ``<package>-<version>.dist-info``
    or ``<package>-<version>.egg-info`` directory. There are specifically named
    files that contain certain data. For example, a ``*.dist-info/METADATA`` file
    describes high-level metadata about a Python package.


    .. py:attribute:: package

        (``string``)

        Python package this resource is associated with.

    .. py:attribute:: name

        (``string``)

        Name of this resource.

    .. py:attribute:: is_stdlib

        (``bool``)

        Whether this module is part of the Python standard library (part of the
        Python distribution).

    .. py:attribute:: add_*

        (various)

        See :ref:`config_resource_add_attributes`.
