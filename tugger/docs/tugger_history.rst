.. _tugger_history:

===============
Project History
===============

.. _tugger_version_history:

Version History
===============

.. _tugger_version_0_3_0:

0.3.0
-----

Not yet released.

New Features
^^^^^^^^^^^^

* The ``WiXMSIBuilder`` Starlark type now implements ``build()`` so it can be
  used as a target.
* The ``WiXMSIBuilder`` Starlark type now exposes ``msi_filename`` and
  ``target_triple`` attributes to customize the output filename and the
  architecture the MSI is built for, respectively.
* Starlark now exposes a ``WiXBundleBuilder`` type to allow the creation of
  *bundle installers* using the WiX Toolset.

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
