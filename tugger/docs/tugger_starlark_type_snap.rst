.. py:currentmodule:: starlark_tugger

========
``Snap``
========

.. py:class:: Snap

    The ``Snap`` type represents an entire ``snapcraft.yaml`` file.

    See https://snapcraft.io/docs/snapcraft-yaml-reference for more documentation.

    Instances of ``Snap`` expose attributes that map to the keys within
    ``snapcraft.yaml`` files.

    Currently the attributes are write only.

    Setting an attribute value to ``None`` has the side-effect of removing that
    attribute from the serialized ``snapcraft.yaml`` file.

    See https://snapcraft.io/docs/snapcraft-yaml-reference for detailed
    documentation about what each attribute means.

    .. py:method:: __init__(name: str, version: str, summary: str, description: str)

        Creates an instance initialized with required parameters. It accepts
        the following arguments:

        ``name``
        ``version``
        ``summary``
        ``description``

    .. py:attribute:: adopt_info

        (``Optional[str]``)

    .. py:attribute:: apps

        (``Optional[dict[str, SnapApp]]``)

    .. py:attribute:: architectures

        (``Optional[dict["build_on" | "run_on", str]]``)

    .. py:attribute:: assumes

        (``Optional[list[str]]``)

    .. py:attribute:: base

        (``Optional[str]``)

    .. py:attribute:: confinement

        (``Optional[str]``)

    .. py:attribute:: description

        (``str``)

    .. py:attribute:: grade

        (``Optional[str]``)

    .. py:attribute:: icon

        (``Optional[str]``)

    .. py:attribute:: license

        (``Optional[str]``)

    .. py:attribute:: name

        (``str``)

    .. py:attribute:: passthrough

        (``Optional[dict[str, str]]``)

    .. py:attribute:: parts

        (``Optional[dict[str, SnapPart]]``)

    .. py:attribute:: plugs

        (``Optional[dict[str, list[str]]]``)

    .. py:attribute:: slots

        (``Optional[dict[str, list[str]]]``)

    .. py:attribute:: summary

        (``str``)

    .. py:attribute:: title

        (``Optional[str]``)

    .. py:attribute:: type

        (``Optional[str]``)

    .. py:attribute:: version

        (``str``)

    .. py:method:: to_builder() -> SnapcraftBuilder

        Converts this instance into a :py:class:`SnapcraftBuilder`.

        This method accepts no arguments and is equivalent to calling
        ``SnapcraftBuilder(self)``.
