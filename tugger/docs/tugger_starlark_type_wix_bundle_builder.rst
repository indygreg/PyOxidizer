.. _tugger_starlark_type_wix_bundle_builder:

====================
``WiXBundleBuilder``
====================

The ``WiXBundleBuilder`` type allows building simple *bundle* installers
with the  `WiX Toolset <https://wixtoolset.org/>`_.

``WiXBundleBuilder`` instances allow you to create ``.exe`` installers that are
composed of a chain of actions. At execution time, each action in the chain is
evaluated. See the WiX Toolset documentation for more.

.. _tugger_starlark_type_wix_bundle_builder_constructors:

Constructors
============

``WiXBundleBuilder()``
----------------------

``WiXBundleBuilder()`` is called to construct new instances. It accepts
the following arguments:

``id_prefix``
   (``string``) The string prefix to add to auto-generated IDs in the ``.wxs``
   XML.

   The value must be alphanumeric and ``-`` cannot be used.

   The value should reflect the application whose installer is being
   defined.

``name``
   (``string``) The name of the application being installed.

``version``
   (``string``) The version of the application being installed.

   This is a string like ``X.Y.Z``, where each component is an integer.

``manufacturer``
   (``string``) The author of the application.

.. _tugger_starlark_type_wix_bundle_builder_methods:

Methods
=======

Sections below document methods available on ``WiXBundleBuilder`` instances.

.. _tugger_starlark_type_wix_bundle_builder.add_vc_redistributable:

``WiXBundleBuilder.add_vc_redistributable()``
---------------------------------------------

This method registers the Visual C++ Redistributable to be installed.

This method accepts the following arguments:

``platform``
   (``string``) The architecture to install for. Valid values are ``x86``,
   ``x64``, and ``arm64``.

The bundle can contain Visual C++ Redistributables for multiple runtime
architectures. The bundle installer will only install the Redistributable
when running on a machine of that architecture. This allows a single bundle
installer to target multiple architectures.

.. _tugger_starlark_type_wix_bundle_builder.build:

``WiXBundleBuilder.build()``
----------------------------

This method will build an exe using the WiX Toolset.

This method accepts the following arguments:

``target``
   (``string``) The name of the target being built.
