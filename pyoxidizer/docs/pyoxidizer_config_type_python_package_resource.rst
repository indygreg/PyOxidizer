.. py:currentmodule:: starlark_pyoxidizer

=========================
``PythonPackageResource``
=========================

.. py:class:: PythonPackageResource

    This type represents a resource _file_ in a Python package. It is
    effectively a named blob associated with a Python package. It is
    typically accessed using the ``importlib.resources`` API.

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
