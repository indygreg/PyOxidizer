.. _tugger_starlark_type_wix_installer:

================
``WiXInstaller``
================

The ``WiXInstaller`` type represents a Windows installer built with the
`WiX Toolset <https://wixtoolset.org/>`_.

``WiXInstaller`` instances allow you to collect ``.wxs`` files for
processing and to turn these into an installer using the ``light.exe`` tool
in the WiX Toolset.

.. _tugger_starlark_wix_installer_constructors:

Constructors
============

``WiXInstaller()``
------------------

``WiXInstaller()`` is called to construct a new instance. It accepts
the following arguments:

``id``
   (``string``) The name of the installer being built.

   This value is used in ``Id`` attributes in WiX XML files and must
   conform to limitations imposed by WiX. Notably, this must be alphanumeric
   and ``-`` cannot be used.

   This value is also used to derive GUIDs for the installer.

   This value should reflect the name of the entity being installed and should
   be unique to prevent collisions with other installers.

.. _tugger_starlark_wix_installer_methods:

Methods
=======

Sections below document methods available on ``WiXInstaller`` instances.

.. _tugger_starlark_wix_installer_add_build_files:

``add_build_files()``
---------------------

This method registers additional files to make available to the build
environment. Files will be materialized next to ``.wxs`` files that will
be processed as part of building the installer.

Accepted arguments are:

``manifest``
   (``FileManifest``) The file manifest defining additional files to
   install.

.. _tugger_starlark_wix_installer_add_simple_installer:

``add_simple_installer()``
--------------------------

This method will populate the installer configuration with a pre-defined
and simple/basic configuration suitable for simple applications. The added
``.wxs`` will produce an MSI that materializes files in the ``Program Files``
directory.

Accepted arguments are:

``product_name``
   (``string``) The name of the installed product. This becomes the value
   of the ``<Product Name="...">`` attribute in the generated ``.wxs`` file.

``product_version``
   (``string``) The version string of the installed product. This becomes
   the value of the ``<Product Version="...">`` attribute in the generated
   ``.wxs`` file.

``product_manufacturer``
   (``string``) The author of the product. This becomes the value of the
   ``<Product Manufacturer="...">`` attribute in the generated ``.wxs`` file.

``program_files``
   (``FileManifest``) Files to materialize in the ``Program Files/<product_name>``
   directory upon install.

.. _tugger_starlark_wix_installer_add_wxs_file:

``add_wxs_file()``
------------------

Adds an existing ``.wxs`` file to be processed as part of building this
installer.

Accepted arguments are:

``path``
   (``string``) The filesystem path to the ``.wxs`` file to add. The file will be
   copied into a temporary directory as part of building the installer and the
   destination filename will be the same as the file's name.

``preprocessor_parameters``
   (``Optional[dict[string, string]]``) Preprocessor parameters to define when
   invoking ``candle.exe`` for this ``.wxs`` file. These effectively constitute
   ``-p`` arguments to ``candle.exe``.

.. _tugger_starlark_wix_installer_set_variable:

``set_variable()``
------------------

Defines a variable to be passed to ``light.exe`` as ``-d`` arguments.

Accepted arguments are:

``key``
   (``string``) The name of the variable.

``value``
   (``Optional[string]``) The value of the variable. If ``None`` is used,
   the variable has no value and is simply defined.
