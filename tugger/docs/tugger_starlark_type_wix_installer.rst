.. _tugger_starlark_type_wix_installer:

================
``WiXInstaller``
================

The ``WiXInstaller`` type represents a Windows installer built with the
`WiX Toolset <https://wixtoolset.org/>`_.

``WiXInstaller`` instances allow you to collect ``.wxs`` files for
processing and to turn these into an installer using the ``light.exe`` tool
in the WiX Toolset.

.. _tugger_starlark_type_wix_installer_constructors:

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

``filename``
   (``string``) The name of the file that will be built.

   WiX supports generating multiple installer file types depending on the
   content of the ``.wxs`` files. You will have to provide a filename that
   is appropriate for the installer type.

   File extensions of ``.msi`` and ``.exe`` are common. If using
   ``add_simple_installer()``, you will want to provide an ``.msi`` filename.

.. _tugger_starlark_type_wix_installer_methods:

Methods
=======

Sections below document methods available on ``WiXInstaller`` instances.

.. _tugger_starlark_type_wix_installer_add_build_files:

``WiXInstaller.add_build_files()``
----------------------------------

This method registers additional files to make available to the build
environment. Files will be materialized next to ``.wxs`` files that will
be processed as part of building the installer.

Accepted arguments are:

``manifest``
   (``FileManifest``) The file manifest defining additional files to
   install.

.. _tugger_starlark_type_wix_installer.add_build_file:

``WiXInstaller.add_build_file()``
---------------------------------

This method registers a single additional file to make available to the
build environment.

Accepted arguments are:

``build_path``
   (``string``) The relative path to materialize inside the build environment

``filesystem_path``
   (``string``) The filesystem path of the file to copy into the build environment.

``force_read``
   (``bool``) Whether to read the content of this file into memory when this
   function is called.

   Defaults to ``False``.

.. _tugger_starlark_type_wix_installer_add_install_file:

``WiXInstaller.add_install_file()``
-----------------------------------

Add a file from the filesystem to be installed by the installer.

This methods accepts the following arguments:

``install_path``
   (``string``) The relative path to materialize inside the installation
   directory.

``filesystem_path``
   (``string``) The filesystem path of the file to install.

``force_read``
   (``bool``) Whether to read the content of this file into memory when this
   function is called.

   Defaults to ``False``.

.. _tugger_starlark_type_wix_installer_add_install_files:

``WiXInstaller.add_install_files()``
------------------------------------

Add files defined in a :ref:`tugger_starlark_type_file_manifest` to be installed
by the installer.

This method accepts the following arguments:

``manifest``
   (``FileManifest``) A :ref:`tugger_starlark_type_file_manifest` defining files
   to materialize in the installation directory. All these files will be installed
   by the installer.

.. _tugger_starlark_type_wix_installer_add_msi_builder:

``WiXInstaller.add_msi_builder()``
----------------------------------

This method adds a :ref:`tugger_starlark_type_wix_msi_builder` instance to this
instance, marking it for processing/building.

This method accepts the following arguments:

``builder``
   (``WiXMSIBuilder``) A :ref:`tugger_starlark_type_wix_msi_builder` representing
   a ``.wxs`` file to build.

.. _tugger_starlark_type_wix_installer_add_simple_installer:

``WiXInstaller.add_simple_installer()``
---------------------------------------

This method will populate the installer configuration with a pre-defined
and simple/basic configuration suitable for simple applications. This method
effectively derives a ``.wxs`` which will produce an MSI that materializes
files in the ``Program Files`` directory.

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

.. _tugger_starlark_type_wix_installer_add_wxs_file:

``WiXInstaller.add_wxs_file()``
-------------------------------

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

.. _tugger_starlark_type_wix_installer_set_variable:

``WiXInstaller.set_variable()``
-------------------------------

Defines a variable to be passed to ``light.exe`` as ``-d`` arguments.

Accepted arguments are:

``key``
   (``string``) The name of the variable.

``value``
   (``Optional[string]``) The value of the variable. If ``None`` is used,
   the variable has no value and is simply defined.
