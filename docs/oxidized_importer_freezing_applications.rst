.. _oxidized_importer_freezing:

==================================================
*Freezing* Applications with ``oxidized_importer``
==================================================

``oxidized_importer`` can be used to create and run *frozen* Python
applications, where Python resources data (module source and bytecode,
etc) is *frozen*/packaged and distributed next to your *application*.

This is conceptually similar to what PyOxidizer does. The major
difference is that PyOxidizer will package and distribute a Python
distribution with your application: when only ``oxidized_importer`` is being
used, the Python distribution is provided by some other means (it is
typically already installed on the system). This makes ``oxidized_importer``
a light-weight alternative to PyOxidizer for scenarios where PyOxidizer
isn't suitable or viable.

High-Level Freezing Workflow
============================

The steps for *freezing* an application all look the same:

1. Load ``OxidizedResource`` instances into an ``OxidizedFinder`` instance
   so they are indexed.
2. Serialize indexed resources.
3. Write the serialized resources blob somewhere along with any
   files (if using filesystem-based loading).
4. Somehow make that resources blob available to others (you could
   add it as a *resource* file in your Python package for example).
5. From your application, construct an ``OxidizedFinder`` instance and
   load the resources blob you generated.
6. Register the ``OxidizedFinder`` instance as the first element on
   ``sys.meta_path``.

The next sections show what this may look like.

.. _oxidized_importer_freezing_build:

Indexing and Serializing Resources
==================================

In your *build* process, you'll need to index resources and serialize
them. You can construct ``OxidizedResource`` instances directly and hand
them off to an ``OxidizedFinder`` instance. But you'll probably want to
use ``OxidizedResourceCollector`` to make this simpler.

Try something like the following:

.. code-block:: python

   import os
   import stat
   import sys

   import oxidized_importer

   # Create a collector to help with managing resources.
   collector = oxidized_importer.OxidizedResourceCollector(policy="in-memory-only")

   # Add all known Python resources by scanning sys.path.
   # Note: this will pull in the Python standard library and
   # any other installed packages, which may not be desirable!
   for path in sys.path:
       # Only directories can be scanned by oxidized_importer.
       if os.path.isdir(path):
           for resource in oxidized_importer.find_resources_in_path(path):
               collector.add_in_memory(resource)

   # Turn the collected resources into ``OxidizedResource`` and file
   # install rules.
   resources, file_installs = collector.oxidize()

   # Now index the resources so we can serialize them.
   finder = oxidized_importer.OxidizedFinder()
   finder.add_resources(resources)

   # Turn the indexed resources into an opaque blob.
   packed_data = finder.serialize_indexed_resources()

   # Write out that data somewhere.
   with open("oxidized_resources", "wb") as fh:
       fh.write(packed_data)

   # Then for all the file installs, materialize those files.
   for (path, data, executable) in file_installs:
       path.parent.mkdir(parents=True, exist_ok=True)

       with path.open("wb") as fh:
           fh.write(data)

       if executable:
           path.chmod(path.stat().st_mode | stat.S_IEXEC)

At this point, you've collected all known Python resources and written
out a data structure describing them all. For resources targeting in-memory
loading, the content of those resources is embedded in the data structure.
For resources targeting filesystem-relative loading, the data structure
contains the relative path to those resources. And you've written out the
files in the locations where those relative paths point to.

Loading Serialized Resources in Your Application
================================================

Now, from our *application* code, we need to load the resources
and register the custom importer with Python:

.. code-block:: python

   import os
   import sys

   import oxidized_importer

   # Load those resources into an instance of our custom importer. This
   # will read the index in the passed data structure and make all
   # resources immediately available for importing.
   finder = oxidized_importer.OxidizedFinder(resources_file="oxidized_resources")

   # If the relative path of filesystem-based resources is not relative
   # to the current executable (which is likely the ``python3`` executable),
   # you'll need to set ``origin`` to the directory the resources are
   # relative to.
   finder = oxidized_importer.OxidizedFinder(
       resources=packed_data,
       relative_path_origin=os.path.dirname(os.path.abspath(__file__)),
   )

   # Register the meta path finder as the first item, making it the
   # first finder that is consulted.
   sys.meta_path.insert(0, finder)

   # At this point, you should be able to ``import`` modules defined
   # in the resources data!
