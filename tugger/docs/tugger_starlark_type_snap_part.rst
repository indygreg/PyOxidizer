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

        (``Optional[list[string]]``)

    .. py:attribute:: build_attributes

        (``Optional[list[string]]``)

    .. py:attribute:: build_environment

        (``Optional[list[dict[string, string]]]``)

    .. py:attribute:: build_packages

        (``Optional[list[string]]``)

    .. py:attribute:: build_snaps

        (``Optional[list[string]]``)

    .. py:attribute:: filesets

        (``Optional[dict[string, list[string]]]``)

    .. py:attribute:: organize

        (``Optional[dict[string, string]]``)

    .. py:attribute:: override_build

        (``Optional[string]``)

    .. py:attribute:: override_prime

        (``Optional[string]``)

    .. py:attribute:: override_pull

        (``Optional[string]``)

    .. py:attribute:: override_stage

        (``Optional[string]``)

    .. py:attribute:: parse_info

        (``Optional[string]``)

    .. py:attribute:: plugin

        (``Optional[string]``)

    .. py:attribute:: prime

        (``Optional[list[string]]``)

    .. py:attribute:: source_branch

        (``Optional[string]``)

    .. py:attribute:: source_checksum

        (``Optional[string]``)

    .. py:attribute:: source_commit

        (``Optional[string]``)

    .. py:attribute:: source_depth

        (``Optional[int]``)

    .. py:attribute:: source_subdir

        (``Optional[string]``)

    .. py:attribute:: source_tag

        (``Optional[string]``)

    .. py:attribute:: source_type

        (``Optional[string]``)

    .. py:attribute:: source

        (``Optional[string]``)

    .. py:attribute:: stage_packages

        (``Optional[list[string]]``)

    .. py:attribute:: stage_snaps

        (``Optional[list[string]]``)

    .. py:attribute:: stage

        (``Optional[list[string]]``)
