.. _oxidized_importer_resource_scanning_apis:

======================
Resource Scanning APIs
======================

The ``oxidized_importer`` module exposes functions and Python types to
facilitate scanning for and collecting Python resources.

.. _find_resources_in_path:

``find_resources_in_path(path)``
================================

The ``oxidized_importer.find_resources_in_path()`` function will scan the
specified filesystem path and return an iterable of objects representing
found resources. Those objects will be 1 of the types documented in
:ref:`oxidized_importer_python_resource_types`.

Only directories can be scanned.

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

The ``oxidized_importer.OxidizedResourceCollector`` type provides functionality
for turning instances of Python resource types into a collection
of ``OxidizedResource`` for loading into an ``OxidizedFinder`` instance. It
exists as a convenience, as working with individual ``OxidizedResource``
instances can be rather cumbersome.

Instances can be constructed by passing a ``policy=<str>`` argument defining
the resources policy for this collector. The string values are the same
as recognized by PyOxidizer's config files and are documented at
:ref:`config_python_resources_policy`.

e.g. to create a collector that only marks resources for in-memory loading:

.. code-block:: python

   import oxidized_importer

   collector = oxidized_importer.OxidizedResourceCollector(policy="in-memory-only")

Instances of ``OxidizedResourceCollector`` have the following properties:

``policy`` (``str``)
   Exposes the policy string this instance was constructed with. This property
   is read-only.

Methods are documented in the following sections.

``add_in_memory(resource)``
---------------------------

``OxidizedResourceCollector.add_in_memory(resource)`` adds a Python resource
type (``PythonModuleSource``, ``PythonModuleBytecode``, etc) to the collector
and marks it for loading via in-memory mechanisms.

``add_filesystem_relative(prefix, resource)``
---------------------------------------------

``OxidizedResourceCollector.add_filesystem_relative(prefix, resource)`` adds a
Python resource type (``PythonModuleSource``, ``PythonModuleBytecode``, etc) to
the collector and marks it for loading via a relative path next to some
*origin* path (as specified to the ``OxidizedFinder``). That relative path
can have a ``prefix`` value prepended to it. If no prefix is desired and you
want the resource placed next to the *origin*, use an empty ``str`` for
``prefix``.

``oxidize()``
-------------

``OxidizedResourceCollector.oxidize()`` takes all the resources collected so
far and turns them into data structures to facilitate later use.

The return value is a tuple of
``(List[OxidizedResource], List[Tuple[pathlib.Path, bytes, bool]])``.

The first element in the tuple is a list of ``OxidizedResource`` instances.

The second is a list of 3-tuples containing the relative filesystem
path for a file, the content to write to that path, and whether the file
should be marked as executable.
