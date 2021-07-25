.. py:currentmodule:: starlark_tugger

=================
``WiXMSIBuilder``
=================

.. py:class:: WiXMSIBuilder

    The ``WiXMSIBuilder`` type allows building simple MSI installers using the
    `WiX Toolset <https://wixtoolset.org/>`_.

    ``WiXMSIBuilder`` instances allow you to create and build a ``.wxs`` file with
    common features. A goal of this type is to allow simple applications - without
    complex installer needs - to generate MSI installers without having to author
    your own ``.wxs`` files.

    Instances have multiple attributes, which are write-only.

    .. py:method:: __init__(id_prefix: str, product_name: str, product_version: str, product_manufacturer: str, arch: str = "x64") -> WiXMSIBuilder

        ``WiXMSIBuilder()`` is called to construct new instances. It accepts
        the following arguments:

        ``id_prefix``
           The string prefix to add to auto-generated IDs in the ``.wxs``
           XML.

           The value must be alphanumeric and ``-`` cannot be used.

           The value should reflect the application whose installer is being
           defined.

        ``product_name``
           The name of the application being installed.

        ``product_version``
           The version of the application being installed.

           This is a string like ``X.Y.Z``, where each component is an integer.

        ``product_manufacturer``
           The author of the application.

        ``arch``
           The WiX architecture of the installer.

    .. py:attribute:: arch

        (``str``)

        The WiX architecture of the installer.

        No validation is performed that the value is a valid WiX architecture or
        that the content of the installer matches the provided architecture.

    .. py:attribute:: banner_bmp_path

        (``str``)

        The path to a 493 x 58 pixel BMP file providing the banner to display in
        the installer.

    .. py:attribute:: dialog_bmp_path

        (``str``)

        The path to a 493 x 312 pixel BMP file providing an image to be displayed in
        the installer.

    .. py:attribute:: eula_rtf_path

        (``str``)

        The path to a RTF file containing the EULA that will be shown to users during
        installation.

    .. py:attribute:: help_url

        (``str``)

        A URL that will be presented to provide users with help.

    .. py:attribute:: license_path

        (``str``)

        Path to a file containing the license for the application being installed.

    .. py:attribute:: msi_filename

        (``str``)

        The filename to use for the built MSI.

        If not set, the default is ``<product_name>-<product_version>.msi``.

    .. py:attribute:: package_description

        (``str``)

        A description of the application being installed.

    .. py:attribute:: package_keywords

        (``str``)

        Keywords for the application being installed.

    .. py:attribute:: product_icon_path

        (``str``)

        Path to a file providing the icon for the installed application.

    .. py:attribute:: upgrade_code

        (``str``)

        A GUID defining the upgrade code for the application.

        If not provided, a stable GUID derived from the application name will be
        derived automatically.

    .. py:method:: add_program_files_manifest(manifest: FileManifest)

        This method registers the content of a
        :py:class:`FileManifest` to be installed in the *Program Files*
        directory for this application.

        This method accepts the following arguments:

        ``manifest``
           Files to register for installation.

        As files are added, they are checked for code signing compatibility with the
        action ``windows-installer-file-added``.

    .. py:method:: add_visual_cpp_redistributable(redist_version: str, platform: str)

        This method will locate and add the Visual C++ Redistributable runtime DLL
        files (e.g. ``vcruntime140.dll``) to the *Program Files* manifest in the builder,
        effectively materializing these files in the installed file layout.

        This method accepts the following arguments:

        ``redist_version``
           The version of the Visual C++ Redistributable to search for and
           add. ``14`` is the version used for Visual Studio 2015, 2017, and 2019.

        ``platform``
           Identifies the Windows run-time architecture. Must be one of
           the values ``x86``, ``x64``, or ``arm64``.

        This method uses ``vswhere.exe`` to locate the ``vcruntimeXXX.dll`` files inside
        a Visual Studio installation. This should *just work* if a modern version of
        Visual Studio is installed. However, it may fail due to system variance.

    .. py:method:: build(target: str) -> ResolvedTarget

        This method will build an MSI using the WiX Toolset.

        This method accepts the following arguments:

        ``target``
           The name of the target being built.

        Upon successful generation of an installer, the produced installer
        will be assessed for code signing with the ``windows-installer-creation``
        *action*.

    .. py:method:: to_file_content() -> FileContent

        Builds the MSI using the WiX Toolset and returns a :py:class:`FileContent`
        representing the built MSI.

        Upon successful generation of an installer, the produced installer
        will be assessed for code signing with the ``windows-installer-creation``
        *action*.

    .. py:method:: write_to_directory(path: str) -> str

        Builds the MSI using the WiX Toolset and writes that installer to the
        specified directory, returning the absolute path of the written file.

        Absolute paths are treated as-is. Relative paths are relative to the
        current build path.

        Upon successful generation of an installer, the produced installer
        will be assessed for code signing with the ``windows-installer-creation``
        *action*.
