.. _oxidized_importer_common_issues:

=============
Common Issues
=============

Extension Modules Support
=========================

Unlike PyOxidizer, ``OxidizedResourceCollector`` isn't (yet) as intelligent
about how to handle extension modules (standalone machine native
shared libraries). And even PyOxidizer's support for extension modules can
be brittle.

One notable difference between PyOxidizer and ``OxidizedResourceCollector``
is PyOxidizer is able to determine whether importing extension modules
from memory is supported and is able to automatically redirect an extension
module to filesystem-based loading if not supported.
``OxidizedResourceCollector`` is *dumb* and adds resources where you tell it
to.

``OxidizedFinder`` supports loading extension modules from memory on Windows.
But everywhere else, this isn't supported and will result in an
``ImportError`` if you index an extension module for in-memory loading.

To work around this deficiency, you'll want to mark extension modules as
loaded from the filesystem unless you are on Windows. Try something
like this:

.. code-block:: python

   import oxidized_importer

   collector = oxidized_importer.OxidizedResourceCollector(
       policy="prefer-in-memory-fallback-filesystem-relative:"
   )

   # Redirect extension modules to the filesystem and everything else to
   # memory.
   for resource in oxidized_importer(find_resources_in_path("/path/to/resources")):
       if isinstance(resource, oxidized_importer.PythonExtensionModule):
           collector.add_filesystem_relative("lib", resource)
       else:
           collector.add_in_memory(resource)

Resource Scanning Descends Into ``site-packages``
=================================================

``oxidized_importer.find_resources_in_path()`` descends into ``site-packages``
directories. This is arguably not the desired behavior, especially when
in the context of virtualenvs, which may want to not inherit the resources
in the ``site-packages`` of the *outer* Python installation. This will
likely be fixed in a future release.
