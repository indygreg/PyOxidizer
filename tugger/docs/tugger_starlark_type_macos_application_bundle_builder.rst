.. py:currentmodule:: starlark_tugger

=================================
``MacOsApplicationBundleBuilder``
=================================

.. py:class:: MacOsApplicationBundleBuilder

    The ``MacOsApplicationBundleBuilder`` type allows creating *macOS Application
    Bundles* (typically ``.app`` directories) providing applications on macOS.

    For reference, see
    `Apple's bundle format documentation <https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/BundleTypes/BundleTypes.html#//apple_ref/doc/uid/10000123i-CH101-SW1>`_
    for the structure of application bundles.

    .. py:method:: __init__(bundle_name: str) -> MacOsApplicationBundleBuilder

        Construct new instances.
        It accepts the following arguments:

        ``bundle_name``
           The name of the application bundle.

           This will become the value for ``CFBundleName`` and form the name of the
           generated bundle directory.

    .. py:method:: add_icon(path: str)

        Accepts a ``string`` argument defining the path to a file that will become the
        ``<bundle_name>.icns`` file for the bundle.

    .. py:method:: add_manifest(manifest: FileManifest)

        Adds file data to the bundle via a :py:class:`FileManifest` instance. All
        files in the manifest will be materialized in the ``Contents/`` directory
        of the bundle.

        Accepts the following arguments:

        ``manifest``
           Collection of files to materialize.

        Bundles have a well-defined structure and files should only be materialized
        in certain locations. This method will allow you to materialize files in
        locations resulting in a malformed bundle. Use with caution.

    .. py:method:: add_macos_file(content: FileContent, path: Optional[str] = None)

        Adds a single file to be installed in the ``Contents/MacOS`` directory in
        the bundle.

        Accepts the following arguments:

        ``content``
           Object representing file content to materialize.

        ``path``
           Relative path of file under ``Contents/MacOS``. If not defined, the file
           will be installed into the equivalent of
           ``os.path.join("Contents/MacOS", content.filename)``.

    .. py:method:: add_macos_manifest(manifest: FileManifest))

        Adds a :py:class:`FileManifest` of content to be materialized in the
        ``Contents/MacOS`` directory.

        Accepts the following arguments:

        ``manifest``
           Collection of files to materialize.

    .. py:method:: add_resources_file(content: FileContent, path: Optional[str])

        Adds a single file to be installed in the ``Contents/Resources`` directory in
        the bundle.

        Accepts the following arguments:

        ``content``
           Object representing file content to materialize.

        ``path``
           Relative path of file under ``Contents/Resources``. If not defined, the file
           will be installed into the equivalent of
           ``os.path.join("Contents/Resources", content.filename)``.

    .. py:method:: add_resources_manifest(manifest: FileManifest)

        Adds a :py:class:`FileManifest` of content to be materialized in the
        ``Contents/Resources`` directory.

        Accepts the following arguments:

        ``manifest``
           Collection of files to materialize.

    .. py:method:: set_info_plist_key(key: str, value: Union[bool, int, str])

        Sets the value of a key in the ``Contents/Info.plist`` file.

        Accepts the following arguments:

        ``key``
           Key in the ```Info.plist`` file to set.

        ``value``
           Value to set. Can be a ``bool``, ``int``, or ``string``.

    .. py:method:: set_info_plist_required_keys(display_name: str, identifier: str, version: str, signature: str, executable: str)

        This method defines required keys in the ``Contents/Info.plist`` file.

        The following named arguments are accepted and must all be provided:

        ``display_name``
           Sets the bundle display name (``CFBundleDisplayName``).

           This is the name of the application as displayed to users.

        ``identifier``
           Sets the bundle identifier (``CFBundleIdentifer``).

           This is a reverse DNS type identifier. e.g. ``com.example.my_program``.

        ``version``
           Sets the bundle version string (``CFBundleVersion``)

        ``signature``
           Sets the bundle creator OS type code (``CFBundleSignature``).

           The value must be exactly 4 characters.

        ``executable``
           Sets the name of the main executable file (``CFBundleExecutable``).

           This is typically the same name as the bundle.

    .. py:method:: build(target: str)

        This method will materialize the ``.app`` bundle/directory given the settings
        specified.

        This method accepts the following arguments:

        ``target``
           The name of the target being built.

        Upon successful bundle directory creation, the entire bundle is
        considered for code signing with the signing action
        ``macos-application-bundle-creation``. All signable Mach-O files and nested
        bundles should be signed.

    .. py:method:: write_to_directory(path: str)

        This method will materialize the ``.app`` bundle/directory to the specified
        directory.

        Absolute paths are treated as-is. Relative paths are relative to the currently
        configured build path.

        Upon successful bundle directory creation, the entire bundle is
        considered for code signing with the signing action
        ``macos-application-bundle-creation``. All signable Mach-O files and nested
        bundles should be signed.
