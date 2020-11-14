.. _tugger_starlark_type_snapcraft_builder:

====================
``SnapcraftBuilder``
====================

The ``SnapcraftBuilder`` type coordinates the invocation of the ``snapcraft``
command.

.. _tugger_starlark_type_snapcraft_builder_constructors:

Constructors
============

``SnapcraftBuilder()``
----------------------

``SnapcraftBuilder()`` constructs a new instance from a
:ref:`tugger_starlark_type_snap`.

It accepts the following arguments:

``snap``
   (``Snap``) The :ref:`tugger_starlark_type_snap` defining the configuration
   to be used.
