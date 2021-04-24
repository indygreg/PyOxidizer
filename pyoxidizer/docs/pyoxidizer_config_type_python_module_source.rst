.. py:currentmodule:: starlark_pyoxidizer

======================
``PythonModuleSource``
======================

.. py:class:: PythonModuleSource

    This type represents Python source modules, agnostic of location.

    Instances can be constructed via
    :py:meth:`PythonExecutable.make_python_module_source` or by calling
    methods that emit Python resources.

    .. py:attribute:: name

        (``string``)

        Fully qualified name of the module. e.g. ``foo.bar``.

    .. py:attribute:: source

        (``string``)

        The Python source code for this module.

    .. py:attribute:: is_package

        (``bool``)

        Whether this module is also a Python package (or sub-package).

    .. py:attribute:: is_stdlib

        (``bool``)

        Whether this module is part of the Python standard library (part of the
        Python distribution).

    .. py:attribute:: add_*

        (various)

        See :ref:`config_resource_add_attributes`.
