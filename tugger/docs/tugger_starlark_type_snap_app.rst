.. py:currentmodule:: starlark_tugger

===========
``SnapApp``
===========

.. py:class:: SnapApp

    The ``SnapApp`` type represents an application entry in a ``snapcraft.yaml``
    file. Specifically, this type represents the values of ``apps.<app-name>`` keys.

    See https://snapcraft.io/docs/snapcraft-yaml-reference for more documentation.

    Instances of ``SnapApp`` expose attributes that map to the keys within
    ``apps.<app-name>`` entries in ``snapcraft.yaml`` configuration files.

    Currently the attributes are write only.

    Setting an attribute value to ``None`` has the side-effect of removing that
    attribute from the serialized ``snapcraft.yaml`` file.

    See https://snapcraft.io/docs/snapcraft-yaml-reference for detailed
    documentation about what each attribute means.


    .. py:method:: __init__() -> SnapApp

        ``SnapApp()`` creates an empty instance. It accepts no arguments.

    .. py:attribute:: adapter

        (``Optional[str]``)

    .. py:attribute:: autostart

        (``Optional[str]``)

    .. py:attribute:: command_chain

        (``Optional[list[str]]``)

    .. py:attribute:: command

        (``Optional[str]``)

    .. py:attribute:: common_id

        (``Optional[str]``)

    .. py:attribute:: daemon

        (``Optional[str]``)

    .. py:attribute:: desktop

        (``Optional[str]``)

    .. py:attribute:: environment

        (``Optional[list[str]]``)

    .. py:attribute:: extensions

        (``Optional[list[str]]``)

    .. py:attribute:: listen_stream

        (``Optional[str]``)

    .. py:attribute:: passthrough

        (``Optional[dict[str, str]]``)

    .. py:attribute:: plugs

        (``Optional[list[str]]``)

    .. py:attribute:: post_stop_command

        (``Optional[str]``)

    .. py:attribute:: restart_condition

        (``Optional[str]``)

    .. py:attribute:: slots

        (``Optional[list[str]]``)

    .. py:attribute:: stop_command

        (``Optional[str]``)

    .. py:attribute:: stop_timeout

        (``Optional[str]``)

    .. py:attribute:: timer

        (``Optional[str]``)

    .. py:attribute:: socket_mode

        (``Optional[int]``)

    .. py:attribute:: socket

        (``Optional[dict[str]]``)
