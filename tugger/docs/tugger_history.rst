.. _tugger_history:

===============
Project History
===============

.. _tugger_version_history:

Version History
===============

.. _tugger_version_0_4_0:

0.4.0
-----

Not yet released.

.. _tugger_version_0_3_0:

0.3.0
-----

Released March 4, 2021.

New Features
^^^^^^^^^^^^

* The ``FileManifest`` Starlark type now exposes an ``add_path()`` method.
* The Starlark dialect now exposes ``SnapApp``, ``Snappart``, and ``Snap`` types
  representing Snapcraft configuration files.
* The Starlark dialect now has a ``SnapcraftBuilder`` type that serves as an
  interface to invoking ``snapcraft``.
* The Starlark dialect now exposes ``WiXBundleBuilder``, ``WiXInstaller``,
  and ``WiXMSIBuilder`` types for defining Windows installers using the WiX
  Toolset.

.. _tugger_version_0_2_0:

0.2.0
-----

Version 0.2 was released November 8, 2020.

Version 0.2 marked the beginning of a complete rewrite of Tugger. The
canonical source code repository was moved to the PyOxidizer repository.

Not all features from version 0.1 were ported to version 0.2.

.. _tugger_version_0_1_0:

0.1.0
-----

Version 0.1 was released on August 25, 2019.

Version 0.1 was mostly a proof of concept to demonstrate the viability
of Starlark configuration files. But Tugger was usable in this release.
