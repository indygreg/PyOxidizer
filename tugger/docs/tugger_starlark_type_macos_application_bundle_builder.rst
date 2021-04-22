.. py:currentmodule:: starlark_tugger

.. _tugger_starlark_type_macos_application_bundle_builder:

=================================
``MacOsApplicationBundleBuilder``
=================================

The ``MacOsApplicationBundleBuilder`` type allows creating *macOS Application
Bundles* (typically ``.app`` directories) providing applications on macOS.

For reference, see
`Apple's bundle format documentation <https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/BundleTypes/BundleTypes.html#//apple_ref/doc/uid/10000123i-CH101-SW1>`_
for the structure of application bundles.

.. _tugger_starlark_type_macos_application_bundle_builder_constructors:

Constructors
============

``MacOsApplicationBundleBuilder()``
-----------------------------------

``MacOsApplicationBundleBuilder()`` is called to construct new instances.
It accepts the following arguments:

``bundle_name``
   (``string``) The name of the application bundle.

   This will become the value for ``CFBundleName`` and form the name of the
   generated bundle directory.

.. _tugger_starlark_type_macos_application_bundle_builder_methods:

Methods
=======

Sections below document methods available on ``MacOsApplicationBundleBuilder``
instances.

.. _tugger_starlark_type_macos_application_bundle_builder.add_icon:

``MacOsApplicationBundleBuilder.add_icon()``
--------------------------------------------

Accepts a ``string`` argument defining the path to a file that will become the
``<bundle_name>.icns`` file for the bundle.

.. _tugger_starlark_type_macos_application_bundle_builder.add_manifest:

``MacOsApplicationBundleBuilder.add_manifest()``
------------------------------------------------

Adds file data to the bundle via a :py:class:`FileManifest` instance. All files
in the manifest will be materialized in the ``Contents/`` directory of the bundle.

Accepts the following arguments:

``manifest``
   (:py:class:`FileManifest`) Collection of files to materialize.

Bundles have a well-defined structure and files should only be materialized
in certain locations. This method will allow you to materialize files in
locations resulting in a malformed bundle. Use with caution.

.. _tugger_starlark_type_macos_application_bundle_builder.add_macos_file:

``MacOsApplicationBundleBuilder.add_macos_file()``
--------------------------------------------------

Adds a single file to be installed in the ``Contents/MacOS`` directory in
the bundle.

Accepts the following arguments:

``path``
   (``string``) Relative path of file under ``Contents/MacOS``.

``content``
   (:py:class:`FileContent`) Object representing file content
   to materialize.

.. _tugger_starlark_type_macos_application_bundle_builder.add_macos_manifest:

``MacOsApplicationBundleBuilder.add_macos_manifest()``
------------------------------------------------------

Adds a :py:class:`FileManifest` of content to be materialized in the
``Contents/MacOS`` directory.

Accepts the following arguments:

``manifest``
   (:py:class:`FileManifest`) Collection of files to materialize.

.. _tugger_starlark_type_macos_application_bundle_builder.add_resources_file:

``MacOsApplicationBundleBuilder.add_resources_file()``
------------------------------------------------------

Adds a single file to be installed in the ``Contents/Resources`` directory in
the bundle.

Accepts the following arguments:

``path``
   (``string``) Relative path of file under ``Contents/Resources``.

``content``
   (:py:class:`FileContent`) Object representing file content to materialize.

.. _tugger_starlark_type_macos_application_bundle_builder.add_resources_manifest:

``MacOsApplicationBundleBuilder.add_resources_manifest()``
----------------------------------------------------------

Adds a :py:class:`FileManifest` of content to be materialized in the
``Contents/Resources`` directory.

Accepts the following arguments:

``manifest``
   (:py:class:`FileManifest`) Collection of files to materialize.

.. _tugger_starlark_type_macos_application_bundle_builder.set_info_plist_key:

``MacOsApplicationBundleBuilder.set_info_plist_key()``
------------------------------------------------------

Sets the value of a key in the ``Contents/Info.plist`` file.

Accepts the following arguments:

``key``
   (``string``) Key in the ```Info.plist`` file to set.

``value``
   (various) Value to set. Can be a ``bool``, ``int``, or ``string``.

.. _tugger_starlark_type_macos_application_bundle_builder.set_info_plist_required_keys:

``MacOsApplicationBundleBuilder.set_info_plist_required_keys()``
----------------------------------------------------------------

This method defines required keys in the ``Contents/Info.plist`` file.

The following named arguments are accepted and must all be provided:

``display_name``
   (``string``) Sets the bundle display name (``CFBundleDisplayName``).

   This is the name of the application as displayed to users.

``identifier``
   (``string``) Sets the bundle identifier (``CFBundleIdentifer``).

   This is a reverse DNS type identifier. e.g. ``com.example.my_program``.

``version``
   (``string``) Sets the bundle version string (``CFBundleVersion``)

``signature``
   (``string``) Sets the bundle creator OS type code (``CFBundleSignature``).

   The value must be exactly 4 characters.

``executable``
   (``string``) Sets the name of the main executable file
   (``CFBundleExecutable``).

   This is typically the same name as the bundle.

.. _tugger_starlark_type_macos_application_bundle_builder.build:

``MacOsApplicationBundleBuilder.build()``
-----------------------------------------

This method will materialize the ``.app`` bundle/directory given the settings
specified.

This method accepts the following arguments:

``target``
   (``string``) The name of the target being built.
