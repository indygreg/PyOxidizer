.. _config_type_file_manifest:

================
``FileManifest``
================

The ``FileManifest`` type represents a set of files and their content.

``FileManifest`` instances are used to represent things like the final
filesystem layout of an installed application.

Conceptually, a ``FileManifest`` is a dict mapping relative paths to
file content.

Methods
=======

.. _config_file_manifest_add_manifest:

``FileManifest.add_manifest()``
-------------------------------

This method overlays another ``FileManifest`` on this one. If the other
manifest provides a path already in this manifest, its content will be
replaced by what is in the other manifest.

``FileManifest.add_python_resource()``
--------------------------------------

This method adds a Python resource to a ``FileManifest`` instance in
a specified directory prefix.

Arguments:

``prefix``
   (``string``) Directory prefix to add resource to.

``value``
   (various) A *Python resource* instance to add. e.g.
   :ref:`config_type_python_module_source` or
   :ref:`config_type_python_package_resource`.

This method can be used to place the Python resources derived from another
type or action in the filesystem next to an application binary.

``FileManifest.add_python_resources()``
---------------------------------------

This method adds an iterable of Python resources to a ``FileManifest``
instance in a specified directory prefix. This is effectively a wrapper
for ``for value in values: self.add_python_resource(prefix, value)``.

For example, to place the Python distribution's standard library Python
source modules in a directory named ``lib``::

   m = FileManifest()
   dist = default_python_distribution()
   for resource in dist.python_resources():
       if type(resource) == "PythonModuleSource":
           m.add_python_resource("lib", resource)

``FileManifest.install()``
--------------------------

This method writes the content of the ``FileManifest`` to a directory
specified by ``path``. The path is evaluated relative to the path
specified by ``BUILD_PATH``.

If ``replace`` is True (the default), the destination directory will
be deleted and the final state of the destination directory should
exactly match the state of the ``FileManifest``.
