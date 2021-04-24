.. py:currentmodule:: oxidized_importer

.. _oxidized_importer_resource_scanning_apis:

======================
Resource Scanning APIs
======================

The ``oxidized_importer`` module exposes functions and Python types to
facilitate scanning for and collecting Python resources.

.. _find_resources_in_path:

``find_resources_in_path(path)``
================================

This function scans a filesystem path and returns discovered resources.
See :py:func:`find_resources_in_path` for the API documentation.

To discover all filesystem based resources that Python's ``PathFinder``
*meta path finder* would (with the exception of ``.zip`` files), try the
following:

.. code-block:: python

   import os
   import oxidized_importer
   import sys

   resources = []
   for path in sys.path:
       if os.path.isdir(path):
           resources.extend(oxidized_importer.find_resources_in_path(path))

``OxidizedResourceCollector`` Python Type
=========================================

The :py:class:`OxidizedResourceCollector` type provides functionality
for turning instances of Python resource types into a collection
of ``OxidizedResource`` for loading into an :py:class:`OxidizedFinder`
instance. It exists as a convenience, as working with individual
``OxidizedResource`` instances can be rather cumbersome.

To create a collector that only marks resources for in-memory loading:

.. code-block:: python

   import oxidized_importer

   collector = oxidized_importer.OxidizedResourceCollector(
       allowed_locations=["in-memory"]
   )
