.. _tugger_starlark_type_snap:

========
``Snap``
========

The ``Snap`` type represents an entire ``snapcraft.yaml`` file.

See https://snapcraft.io/docs/snapcraft-yaml-reference for more documentation.

.. _tugger_starlark_type_snap_constructors:

Constructors
============

``Snap()``
----------

``Snap()`` creates an instance initialized with required parameters. It accepts
the following arguments:

``name``
   (``string``)
``version``
   (``string``)
``summary``
   (``string``)
``description``
   (``string``)

.. _tugger_starlark_type_snap_attributes:

Attributes
==========

Instances of ``Snapt`` expose attributes that map to the keys within ``snapcraft.yaml``
files.

Currently the attributes are write only.

Setting an attribute value to ``None`` has the side-effect of removing that
attribute from the serialized ``snapcraft.yaml`` file.

See https://snapcraft.io/docs/snapcraft-yaml-reference for detailed
documentation about what each attribute means.

``adopt_info``
--------------

(``Optional[string]``)

``apps``
--------

(``Optional[dict[string, SnapApp]]``)

``architectures``
-----------------

(``Optional[dict["build_on" | "run_on", string]]``)

``assumes``
-----------

(``Optional[list[string]]``)

``base``
--------

(``Optional[string]``)

``confinement``
---------------

(``Optional[string]``)

``description``
---------------

(``string``)

``grade``
---------

(``Optional[string]``)

``icon``
--------

(``Optional[string]``)

``license``
-----------

(``Optional[string]``)

``name``
--------

(``string``)

``passthrough``
---------------

(``Optional[dict[string, string]]``)

``parts``
---------

(``Optional[dict[string, SnapPart]]``)

``plugs``
---------

(``Optional[dict[string, list[string]]]``)

``slots``
---------

(``Optional[dict[string, list[string]]]``)

``summary``
-----------

(``string``)

``title``
---------

(``Optional[string]``)

``type``
--------

(``Optional[string]``)

``version``
-----------

(``string``)
