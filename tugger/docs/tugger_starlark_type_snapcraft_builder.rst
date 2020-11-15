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

.. _tugger_starlark_type_snapcraft_builder_methods:

Methods
=======

.. _tugger_starlark_type_snapcraft_builder_add_invocation:

``SnapcraftBuilder.add_invocation()``
-------------------------------------

This method registers an invocation of ``snapcraft`` with the builder. When
this instance is built, all registered invocations will be run sequentially.

The following arguments are accepted:

``args``
   (``List[String]``) Arguments to pass to ``snapcraft`` executable.

``purge_build``
   (``Optional[bool]``) Whether to purge the build directory before running
   this invocation.

   If not specified, the build directory is purged for the first registered
   invocation and not purged for all subsequent invocations.

.. _tugger_starlark_type_snapcraft_builder_add_file_manifest:

``SnapcraftBuilder.add_file_manifest()``
----------------------------------------

This method registers the content of a
:ref:`tugger_starlark_type_file_manifest` with the build environment for
this builder.

When this instance is built, the content of the passed manifest will be
materialized in a directory next to the ``snapcraft.yaml`` file this instance
is building.

The following arguments are accepted:

``manifest``
   (``FileManifest``) A :ref:`tugger_starlark_type_file_manifest` defining
   files to install in the build environment.

.. _tugger_starlark_type_snapcraft_builder_build:

``SnapcraftBuilder.build()``
----------------------------

This method invokes the builder and runs ``snapcraft``.

The following arguments are accepted:

``target``
   (``String``) The name of the build target.

This method returns a ``ResolvedTarget``. That target is not runnable.
