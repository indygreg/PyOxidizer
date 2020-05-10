.. _oxidized_importer:

======================================
``oxidized_importer`` Python Extension
======================================

``oxidized_importer`` is a Python extension module maintained as part of
the PyOxidizer project that allows you to:

* Install a custom, high-performance module importer (``OxidizedFinder``)
  to service Python ``import`` statements and resource loading (potentially
  from memory).
* Scan the filesystem for Python resources (source modules, bytecode
  files, package resources, distribution metadata, etc) and turn them
  into Python objects.
* Serialize Python resource data into an efficient binary data structure
  for loading into an ``OxidizedFinder`` instance. This facilitates
  producing a standalone *resources blob* that can be distributed with
  a Python application which contains all the Python modules, bytecode,
  etc required to power that application.

``oxidized_importer`` is automatically compiled into applications built
with PyOxidizer. It can also be built as a standalone extension module and
used with regular Python installs.

.. toctree::
   :maxdepth: 2

   oxidized_importer_getting_started
   oxidized_importer_meta_path_finders
   oxidized_importer_oxidized_finder
   oxidized_importer_behavior_and_compliance
   oxidized_importer_python_resource_types
   oxidized_importer_resource_scanning
   oxidized_importer_resource_files
   oxidized_importer_freezing_applications
   oxidized_importer_known_issues
   oxidized_importer_security
