.. _tugger_starlark_type_snap_app:

===========
``SnapApp``
===========

The ``SnapApp`` type represents an application entry in a ``snapcraft.yaml``
file. Specifically, this type represents the values of ``apps.<app-name>`` keys.

See https://snapcraft.io/docs/snapcraft-yaml-reference for more documentation.

.. _tugger_starlark_type_snap_app_constructors:

Constructors
============

``SnapApp()``
-------------

``SnapApp()`` creates an empty instance. It accepts no arguments.

.. _tugger_starlark_type_snap_app_attributes:

Attributes
==========

Instances of ``SnapApp`` expose attributes that map to the keys within
``apps.<app-name>`` entries in ``snapcraft.yaml`` configuration files.

Currently the attributes are write only.

Setting an attribute value to ``None`` has the side-effect of removing that
attribute from the serialized ``snapcraft.yaml`` file.

See https://snapcraft.io/docs/snapcraft-yaml-reference for detailed
documentation about what each attribute means.

``adapter``
-----------

(``Optional[string]``)

``autostart``
-------------

(``Optional[string]``)

``command_chain``
-----------------

(``Optional[list[string]]``)

``command``
-----------

(``Optional[string]``)

``common_id``
-------------

(``Optional[string]``)

``daemon``
----------

(``Optional[string]``)

``desktop``
-----------

(``Optional[string]``)

``environment``
---------------

(``Optional[list[string]]``)

``extensions``
--------------

(``Optional[list[string]]``)

``listen_stream``
-----------------

(``Optional[string]``)

``passthrough``
---------------

(``Optional[dict[string, string]]``)

``plugs``
---------

(``Optional[list[string]]``)

``post_stop_command``
---------------------

(``Optional[string]``)

``restart_condition``
---------------------

(``Optional[string]``)

``slots``
---------

(``Optional[list[string]]``)

``stop_command``
----------------

(``Optional[string]``)

``stop_timeout``
----------------

(``Optional[string]``)

``timer``
---------

(``Optional[string]``)

``socket_mode``
---------------

(``Optional[int]``)

``socket``
----------

(``Optional[dict[string]]``)
