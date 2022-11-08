.. py:currentmodule:: oxidized_importer

.. _oxidized_importer_history:

===============
Project History
===============

Changelog
=========

0.9.0
-----

(Not yet released)

0.8.0
-----

Released November 7, 2022.

0.7.0
-----

No release notes.

0.6.0
-----

Released on June 6, 2022.

* Added missing API docs for :py:class:`OxidizedDistribution`.
* ``OxidizedDistribution.metadata`` now returns an
  ``importlib.metadata._adapters.Message`` instance on Python 3.10+.
* ``OxidizedDistribution.entry_points`` now calls
  ``importlib.metadata.EntryPoints._from_text_for`` on Python 3.10+.
  Previously, the implementation of this method didn't work properly on 3.10+.
* Added ``name`` property to :py:class:`OxidizedDistribution`.
* Added ``_normalized_name`` property to :py:class:`OxidizedDistribution`.
* PyO3 Rust crate upgraded to 0.16.5. This gets us better compatibility with
  Python 3.10.
