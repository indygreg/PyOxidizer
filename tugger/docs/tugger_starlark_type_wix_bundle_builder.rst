.. py:currentmodule:: starlark_tugger

====================
``WiXBundleBuilder``
====================

.. py:class:: WiXBundleBuilder

    The ``WiXBundleBuilder`` type allows building simple *bundle* installers
    with the  `WiX Toolset <https://wixtoolset.org/>`_.

    ``WiXBundleBuilder`` instances allow you to create ``.exe`` installers that are
    composed of a chain of actions. At execution time, each action in the chain is
    evaluated. See the WiX Toolset documentation for more.

    .. py:method:: __init__(id_prefix: str, name: str, version: str, manufacturer: str, arch: str = "x64") -> WiXBundleBuilder

        ``WiXBundleBuilder()`` is called to construct new instances. It accepts
        the following arguments:

        ``id_prefix``
           The string prefix to add to auto-generated IDs in the ``.wxs`` XML.

           The value must be alphanumeric and ``-`` cannot be used.

           The value should reflect the application whose installer is being
           defined.

        ``name``
           The name of the application being installed.

        ``version``
           The version of the application being installed.

           This is a string like ``X.Y.Z``, where each component is an integer.

        ``manufacturer``
           The author of the application.

        ``arch``
           The WiX architecture of the installer being built.

    .. py:method:: add_condition(condition: str, message: str)

        Defines a ``<bal:Condition>`` that must be satisfied to run this installer.

        See the WiX Toolkit documentation for more.

        This method accepts the following arguments:

        ``condition``
           The condition expression that must be satisfied.

        ``message``
           The message that will be displayed if the condition is not met.

    .. py:method:: add_vc_redistributable(platform: str)

        This method registers the Visual C++ Redistributable to be installed.

        This method accepts the following arguments:

        ``platform``
           The architecture to install for. Valid values are ``x86``, ``x64``, and
           ``arm64``.

        The bundle can contain Visual C++ Redistributables for multiple runtime
        architectures. The bundle installer will only install the Redistributable
        when running on a machine of that architecture. This allows a single bundle
        installer to target multiple architectures.

    .. py:method:: add_wix_msi_builder(builder: WiXMSIBuilder, display_internal_ui: Optional[bool] = False, install_condition: Optional[str] = None)

        This method adds a :py:class:`WiXMSIBuilder` to be installed
        by the produced installer.

        This method accepts the following arguments:

        ``builder``
           The :py:class:`WiXMSIBuilder` representing an MSI to install.

        ``display_internal_ui``
           Whether to display the UI of the MSI.

        ``install_condition``
           An expression that must be true for this MSI to be installed.

        This method effectively coerces the :py:class:`WiXMSIBuilder` instance to an
        ``<MsiPackage>`` element and adds it to the ``<Chain>`` in the bundle XML.
        See the WiX Toolset documentation for more.

    .. py:method:: build(target: str) -> ResolvedTarget

        This method will build an exe using the WiX Toolset.

        This method accepts the following arguments:

        ``target``
           The name of the target being built.

        Upon successful generation of an installer, the produced installer
        will be assessed for code signing with the ``windows-installer-creation``
        *action*.

    .. py:method:: to_file_content() -> FileContent

        Build an exe installer using the WiX Toolset and return a
        :py:class:`FileContent` representing the built installer.

        Upon successful generation of an installer, the produced installer
        will be assessed for code signing with the ``windows-installer-creation``
        *action*.

    .. py:method:: write_to_directory(path: str) -> str

        Build an exe installer using the WiX Toolset and write the built installer
        to the directory specified, returning the absolute path of the written file.

        Absolute paths are treated as-is. Relative paths are relative to the current
        build path.

        Upon successful generation of an installer, the produced installer
        will be assessed for code signing with the ``windows-installer-creation``
        *action*.
