.. py:currentmodule:: starlark_tugger

============
``SnapPart``
============

.. py:class:: SnapPart

    The ``SnapPart`` type represents a part entry in a ``snapcraft.yaml`` file.
    Specifically, this type represents the values of ``parts.<part-name>`` keys.

    See https://snapcraft.io/docs/snapcraft-yaml-reference for more documentation.

    Instances of ``SnapPart`` expose attributes that map to the keys within
    ``parts.<part-name>`` entries in ``snapcraft.yaml`` configuration files.

    Currently the attributes are write only.

    Setting an attribute value to ``None`` has the side-effect of removing that
    attribute from the serialized ``snapcraft.yaml`` file.

    See https://snapcraft.io/docs/snapcraft-yaml-reference for detailed
    documentation about what each attribute means.

    .. py:method:: __init__() -> SnapPart

        ``SnapPart()`` creates an empty instance. It accepts no arguments.

    .. py:attribute:: after

        (``Optional[list[str]]``)

    .. py:attribute:: build_attributes

        (``Optional[list[str]]``)

    .. py:attribute:: build_environment

        (``Optional[list[dict[str, str]]]``)

    .. py:attribute:: build_packages

        (``Optional[list[str]]``)

    .. py:attribute:: build_snaps

        (``Optional[list[str]]``)

    .. py:attribute:: filesets

        (``Optional[dict[str, list[str]]]``)

    .. py:attribute:: organize

        (``Optional[dict[str, str]]``)

    .. py:attribute:: override_build

        (``Optional[str]``)

    .. py:attribute:: override_prime

        (``Optional[str]``)

    .. py:attribute:: override_pull

        (``Optional[str]``)

    .. py:attribute:: override_stage

        (``Optional[str]``)

    .. py:attribute:: parse_info

        (``Optional[str]``)

    .. py:attribute:: plugin

        (``Optional[str]``)

    .. py:attribute:: prime

        (``Optional[list[str]]``)

    .. py:attribute:: source_branch

        (``Optional[str]``)

    .. py:attribute:: source_checksum

        (``Optional[str]``)

    .. py:attribute:: source_commit

        (``Optional[str]``)

    .. py:attribute:: source_depth

        (``Optional[int]``)

    .. py:attribute:: source_subdir

        (``Optional[str]``)

    .. py:attribute:: source_tag

        (``Optional[str]``)

    .. py:attribute:: source_type

        (``Optional[str]``)

    .. py:attribute:: source

        (``Optional[str]``)

    .. py:attribute:: stage_packages

        (``Optional[list[str]]``)

    .. py:attribute:: stage_snaps

        (``Optional[list[str]]``)

    .. py:attribute:: stage

        (``Optional[list[str]]``)
