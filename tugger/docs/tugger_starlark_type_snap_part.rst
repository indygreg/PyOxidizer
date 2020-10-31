.. _tugger_starlark_type_snap_part:

============
``SnapPart``
============

The ``SnapPart`` type represents a part entry in a ``snapcraft.yaml`` file.
Specifically, this type represents the values of ``parts.<part-name>`` keys.

See https://snapcraft.io/docs/snapcraft-yaml-reference for more documentation.

.. _tugger_starlark_type_snap_part_constructors:

Constructors
============

``SnapPart()``
--------------

``SnapPart()`` creates an empty instance. It accepts no arguments.

.. _tugger_starlark_type_snap_part_attributes:

Attributes
==========

Instances of ``SnapPart`` expose attributes that map to the keys within
``parts.<part-name>`` entries in ``snapcraft.yaml`` configuration files.

Currently the attributes are write only.

Setting an attribute value to ``None`` has the side-effect of removing that
attribute from the serialized ``snapcraft.yaml`` file.

See https://snapcraft.io/docs/snapcraft-yaml-reference for detailed
documentation about what each attribute means.

``after``
---------

(``Optional[list[string]]``)

``build_attributes``
--------------------

(``Optional[list[string]]``)

``build_environment``
---------------------

(``Optional[list[string]]``)

``build_packages``
------------------

(``Optional[list[string]]``)

``build_snaps``
---------------

(``Optional[list[string]]``)

``filesets``
------------

(``Optional[dict[string, list[string]]]``)

``organize``
------------

(``Optional[dict[string, string]]``)

``override_build``
------------------

(``Optional[string]``)

``override_prime``
------------------

(``Optional[string]``)

``override_pull``
-----------------

(``Optional[string]``)

``override_stage``
------------------

(``Optional[string]``)

``parse_info``
--------------

(``Optional[string]``)

``plugin``
----------

(``Optional[string]``)

``prime``
---------

(``Optional[list[string]]``)

``source_branch``
-----------------

(``Optional[string]``)

``source_checksum``
-------------------

(``Optional[string]``)

``source_commit``
-----------------

(``Optional[string]``)

``source_depth``
----------------

(``Optional[int]``)

``source_subdir``
-----------------

(``Optional[string]``)

``source_tag``
--------------

(``Optional[string]``)

``source_type``
---------------

(``Optional[string]``)

``source``
----------

(``Optional[string]``)

``stage_packages``
------------------

(``Optional[list[string]]``)

``stage_snaps``
---------------

(``Optional[list[string]]``)

``stage``
---------

(``Optional[list[string]]``)
