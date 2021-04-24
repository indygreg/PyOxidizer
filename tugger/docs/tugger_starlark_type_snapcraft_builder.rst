.. py:currentmodule:: starlark_tugger

====================
``SnapcraftBuilder``
====================

.. py:class:: SnapcraftBuilder

    The ``SnapcraftBuilder`` type coordinates the invocation of the ``snapcraft``
    command.

    .. py:method:: __init__(snap: Snap) -> SnapcraftBuilder

        ``SnapcraftBuilder()`` constructs a new instance from a :py:class:`Snap`.

        It accepts the following arguments:

        ``snap``
           The :py:class:`Snap` defining the configuration to be used.

    .. py:method:: add_invocation(args: List[str], purge_build: Optional[bool])

        This method registers an invocation of ``snapcraft`` with the builder. When
        this instance is built, all registered invocations will be run sequentially.

        The following arguments are accepted:

        ``args``
           Arguments to pass to ``snapcraft`` executable.

        ``purge_build``
           Whether to purge the build directory before running this invocation.

           If not specified, the build directory is purged for the first registered
           invocation and not purged for all subsequent invocations.

    .. py:method:: add_file_manifest(manifest: FileManifest)

        This method registers the content of a
        :py:class:`FileManifest` with the build environment for
        this builder.

        When this instance is built, the content of the passed manifest will be
        materialized in a directory next to the ``snapcraft.yaml`` file this instance
        is building.

        The following arguments are accepted:

        ``manifest``
           Defines files to install in the build environment.

    .. py:method:: build(target: str) -> ResolvedTarget

        This method invokes the builder and runs ``snapcraft``.

        The following arguments are accepted:

        ``target``
           The name of the build target.

        This method returns a ``ResolvedTarget``. That target is not runnable.
