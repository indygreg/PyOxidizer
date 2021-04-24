.. py:currentmodule:: starlark_tugger

================
``WiXInstaller``
================

.. py:class:: WiXInstaller

    The ``WiXInstaller`` type represents a Windows installer built with the
    `WiX Toolset <https://wixtoolset.org/>`_.

    ``WiXInstaller`` instances allow you to collect ``.wxs`` files for
    processing and to turn these into an installer using the ``light.exe`` tool
    in the WiX Toolset.

    .. py:method:: __init__(id: str, filename: str) -> WiXInstaller

        ``WiXInstaller()`` is called to construct a new instance. It accepts
        the following arguments:

        ``id``
           The name of the installer being built.

           This value is used in ``Id`` attributes in WiX XML files and must
           conform to limitations imposed by WiX. Notably, this must be alphanumeric
           and ``-`` cannot be used.

           This value is also used to derive GUIDs for the installer.

           This value should reflect the name of the entity being installed and should
           be unique to prevent collisions with other installers.

        ``filename``
           The name of the file that will be built.

           WiX supports generating multiple installer file types depending on the
           content of the ``.wxs`` files. You will have to provide a filename that
           is appropriate for the installer type.

           File extensions of ``.msi`` and ``.exe`` are common. If using
           ``add_simple_installer()``, you will want to provide an ``.msi`` filename.

    .. py:method:: add_build_files(manifest: FileManifest)

        This method registers additional files to make available to the build
        environment. Files will be materialized next to ``.wxs`` files that will
        be processed as part of building the installer.

        Accepted arguments are:

        ``manifest``
           The file manifest defining additional files to install.

    .. py:method:: add_build_file(build_path: str, filesystem_path: str, force_read: Optional[bool] = False)

        This method registers a single additional file to make available to the
        build environment.

        Accepted arguments are:

        ``build_path``
           The relative path to materialize inside the build environment

        ``filesystem_path``
           The filesystem path of the file to copy into the build environment.

        ``force_read``
           Whether to read the content of this file into memory when this
           function is called.

    .. py:method:: add_install_file(install_path: str, filesystem_path: str, force_read: Optional[bool] = False)

        Add a file from the filesystem to be installed by the installer.

        This methods accepts the following arguments:

        ``install_path``
           The relative path to materialize inside the installation directory.

        ``filesystem_path``
           The filesystem path of the file to install.

        ``force_read``
           Whether to read the content of this file into memory when this function
           is called.

    .. py:method:: add_install_files(manifest: FileManifest)

        Add files defined in a :py:class:`FileManifest` to be installed by the
        installer.

        This method accepts the following arguments:

        ``manifest``
           Defines files to materialize in the installation directory. All these files
           will be installed by the installer.

    .. py:method:: add_msi_builder(builder: WiXMSIBuilder)

        This method adds a :py:class:`WiXMSIBuilder` instance to this
        instance, marking it for processing/building.

    .. py:method:: add_simple_installer(product_name: str, product_version: str, product_manufacturer: str, program_files: FileManifest)

        This method will populate the installer configuration with a pre-defined
        and simple/basic configuration suitable for simple applications. This method
        effectively derives a ``.wxs`` which will produce an MSI that materializes
        files in the ``Program Files`` directory.

        Accepted arguments are:

        ``product_name``
           The name of the installed product. This becomes the value
           of the ``<Product Name="...">`` attribute in the generated ``.wxs`` file.

        ``product_version``
           The version string of the installed product. This becomes
           the value of the ``<Product Version="...">`` attribute in the generated
           ``.wxs`` file.

        ``product_manufacturer``
           The author of the product. This becomes the value of the
           ``<Product Manufacturer="...">`` attribute in the generated ``.wxs`` file.

        ``program_files``
           Files to materialize in the ``Program Files/<product_name>``
           directory upon install.

    .. py:method:: add_wxs_file(path: str, preprocessor_parameters: Optional[dict[str, str]])

        Adds an existing ``.wxs`` file to be processed as part of building this
        installer.

        Accepted arguments are:

        ``path``
           The filesystem path to the ``.wxs`` file to add. The file will be
           copied into a temporary directory as part of building the installer and the
           destination filename will be the same as the file's name.

        ``preprocessor_parameters``
           Preprocessor parameters to define when invoking ``candle.exe`` for this
           ``.wxs`` file. These effectively constitute ``-p`` arguments to
           ``candle.exe``.

    .. py:method:: set_variable(key: str, value: Optional[str])

        Defines a variable to be passed to ``light.exe`` as ``-d`` arguments.

        Accepted arguments are:

        ``key``
           The name of the variable.

        ``value``
           The value of the variable. If ``None`` is used, the variable has no
           value and is simply defined.
