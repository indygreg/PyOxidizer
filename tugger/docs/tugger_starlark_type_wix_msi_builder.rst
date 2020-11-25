.. _tugger_starlark_type_wix_msi_builder:

=================
``WiXMSIBuilder``
=================

The ``WiXMSIBuilder`` type allows building simple MSI installers using the
`WiX Toolset <https://wixtoolset.org/>`_.

``WiXMSIBuilder`` instances allow you to create and build a ``.wxs`` file with
common features. A goal of this type is to allow simple applications - without
complex installer needs - to generate MSI installers without having to author
your own ``.wxs`` files.

.. _tugger_starlark_type_wix_msi_builder_constructors:

Constructors
============

``WiXMSIBuilder()``
-------------------

``WiXMSIBuilder()`` is called to construct new instances. It accepts
the following arguments:

``id_prefix``
   (``string``) The string prefix to add to auto-generated IDs in the ``.wxs``
   XML.

   The value must be alphanumeric and ``-`` cannot be used.

   The value should reflect the application whose installer is being
   defined.

``product_name``
   (``string``) The name of the application being installed.

``product_version``
   (``string``) The version of the application being installed.

   This is a string like ``X.Y.Z``, where each component is an integer.

``product_manufacturer``
   (``string``) The author of the application.

.. _tugger_starlark_type_wix_msi_builder_attributes:

Attributes
==========

Sections below document attributes available on instances. Attributes
are write only.

``banner_bmp_path``
-------------------

(``string``)

The path to a 493 x 58 pixel BMP file providing the banner to display in
the installer.

``dialog_bmp_path``
-------------------

(``string``)

The path to a 493 x 312 pixel BMP file providing an image to be displayed in
the installer.

``eula_rtf_path``
-----------------

(``string``)

The path to a RTF file containing the EULA that will be shown to users during
installation.

``help_url``
------------

(``string``)

A URL that will be presented to provide users with help.

``license_path``
----------------

(``string``)

Path to a file containing the license for the application being installed.

``msi_filename``
----------------

(``string``)

The filename to use for the built MSI.

If not set, the default is ``<product_name>-<product_version>.msi``.

``package_description``
-----------------------

(``string``)

A description of the application being installed.

``package_keywords``
--------------------

(``string``)

Keywords for the application being installed.

``product_icon_path``
---------------------

(``string``)

Path to a file providing the icon for the installed application.

``target_triple``
-----------------

(``string``)

The Rust target triple the MSI is being built for.

``upgrade_code``
----------------

(``string``)

A GUID defining the upgrade code for the application.

If not provided, a stable GUID derived from the application name will be
derived automatically.

.. _tugger_starlark_type_wix_msi_builder_methods:

Methods
=======

Sections below document methods available on ``WiXMSIBuilder`` instances.

.. _tugger_starlark_type_wix_msi_builder.add_program_files_manifest:

``WiXMSIBuilder.add_program_files_manifest()``
----------------------------------------------

This method registers the content of a
:ref:`tugger_starlark_type_file_manifest` to be installed in the *Program Files*
directory for this application.

This method accepts the following arguments:

``manifest``
   (``FileManifest``) A :ref:`tugger_starlark_type_file_manifest` containing files
   to register for installation.

.. _tugger_starlark_type_wix_msi_builder.add_visual_cpp_redistributable:

``WiXMSIBuilder.add_visual_cpp_redistributable()``
--------------------------------------------------

This method will locate and add the Visual C++ Redistributable runtime DLL
files (e.g. ``vcruntime140.dll``) to the *Program Files* manifest in the builder,
effectively materializing these files in the installed file layout.

This method accepts the following arguments:

``redist_version``
   (``string``) The version of the Visual C++ Redistributable to search for and
   add. ``14`` is the version used for Visual Studio 2015, 2017, and 2019.

``platform``
   (``string``) Identifies the Windows run-time architecture. Must be one of
   the values ``x86``, ``x64``, or ``arm64``.

This method uses ``vswhere.exe`` to locate the ``vcruntimeXXX.dll`` files inside
a Visual Studio installation. This should *just work* if a modern version of
Visual Studio is installed. However, it may fail due to system variance.

.. _tugger_starlark_type_wix_msi_builder.build:

``WiXMSIBuilder.build()``
-------------------------

This method will build an MSI using the WiX Toolset.

This method accepts the following arguments:

``target``
   (``string``) The name of the target being built.
