.. py:currentmodule:: starlark_pyoxidizer

=========================
``PythonExtensionModule``
=========================

.. py:class:: PythonExtensionModule

    This type represents a compiled Python extension module.

    .. py:attribute:: name

        (``string``)

        Unique name of the module being provided.

    .. py:attribute:: is_stdlib

        (``bool``)

        Whether this module is part of the Python standard library (part of the
        Python distribution).

    .. py:attribute:: add_*

        (various)

        See :ref:`config_resource_add_attributes`.
