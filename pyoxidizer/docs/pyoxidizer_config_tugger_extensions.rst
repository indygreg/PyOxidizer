.. py:currentmodule:: starlark_pyoxidizer

.. _config_tugger_extensions:

=======================================
Extensions to Tugger's Starlark Dialect
=======================================

PyOxidizer extends :ref:`Tugger's Starlark dialect <tugger_starlark>`
with addition methods.

.. _config_type_file_manifest.add_python_resource:

``FileManifest.add_python_resource()``
======================================

This method adds a Python resource to a :py:class:`starlark_tugger.FileManifest`
instance in a specified directory prefix.

Arguments:

``prefix``
   (``string``) Directory prefix to add resource to.

``value``
   (various) A *Python resource* instance to add. e.g.
   :py:class:`PythonModuleSource` or :py:class:`PythonPackageResource`.

This method can be used to place the Python resources derived from another
type or action in the filesystem next to an application binary.

.. _config_type_file_manifest.add_python_resources:

``FileManifest.add_python_resources()``
=======================================

This method adds an iterable of Python resources to a
:py:class:`starlark_tugger.FileManifest` instance in a specified directory
prefix. This is effectively a wrapper for
``for value in values: self.add_python_resource(prefix, value)``.

For example, to place the Python distribution's standard library Python
source modules in a directory named ``lib``::

   m = FileManifest()
   dist = default_python_distribution()
   for resource in dist.python_resources():
       if type(resource) == "PythonModuleSource":
           m.add_python_resource("lib", resource)