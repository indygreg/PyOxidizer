.. _oxidized_importer_meta_path_finders:

========================
Python Meta Path Finders
========================

Python allows providing custom Python types to handle the low-level
machinery behind the ``import`` statement. The way this works is a
*meta path finder* instance (as defined by the
`importlib.abc.MetaPathFinder <https://docs.python.org/3/library/importlib.html#importlib.abc.MetaPathFinder>`_
interface) is registered on
`sys.meta_path <https://docs.python.org/3/library/sys.html#sys.meta_path>`_.
When an ``import`` is serviced, Python effectively iterates the objects
on ``sys.meta_path`` and asks each one *can you service this request*
until one does.

These *meta path finder* not only service basic Python module loading,
but they can also facilitate loading resource files and package metadata.
There are a handful of optional methods available on implementations.

This documentation will often refer to a *meta path finder* as an *importer*,
because it is primarily used for *importing* Python modules.

Normally when you start a Python process, the Python interpreter itself
will install 3 *meta path finders* on ``sys.meta_path`` before your
code even has a chance of running:

``BuiltinImporter``
   Handles importing of *built-in* extension modules, which are compiled
   into the Python interpreter. These include modules like ``sys``.
``FrozenImporter``
   Handles importing of *frozen* bytecode modules, which are compiled
   into the Python interpreter. This *finder* is typically only used
   to initialize Python's importing mechanism.
``PathFinder``
   Handles filesystem-based loading of resources. This is what is used
   to import ``.py`` and ``.pyc`` files. It also handles ``.zip`` files.
   This is the *meta path finder* that most imports are traditionally
   serviced by. It queries the filesystem at ``import`` time to find
   and load resources.
