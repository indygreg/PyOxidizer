.. _resource_files:

======================
Loading Resource Files
======================

Many Python application need to load *resources*. *Resources* are typically
non-Python *support* files, such as images, config files, etc. In some cases,
*resources* could be Python source or bytecode files. For example, many
plugin systems load Python modules outside the context of the normal
``import`` mechanism and therefore treat standalone Python source/bytecode
files as non-module *resources*.

``oxidized_importer`` has support for loading resource files. But
compatibility with Python's expected behavior may vary.

Python Resource Loading Mechanisms
==================================

Before we talk about ``oxidized_importer``'s support for resource loading,
it is important to understand how Python code in the wild can load
resources.

We'll overview them in the chronological order they were introduced into
the Python ecosystem.

The most basic and oldest mechanism to load resources is to perform raw
filesystem I/O. Typically, Python code looks at ``__file__`` to get the
filename of the current module. Then, it calculates the directory name and
derives paths to resource files using e.g. ``os.path.join()``. It will
usually then  ``open()`` these paths directly.

Python packaging evolved over time. Packaging tools could express
various metadata at build time, such as supplementary *resource* files.
This metadata would be installed next to a package and APIs could be
used to access it. One such API was
`pkg_resources <https://setuptools.readthedocs.io/en/latest/pkg_resources.html>`_.
Using e.g. ``pkg_resources.resource_string("foo", "bar.txt")``, you could
obtain the content of the resource ``bar.txt`` in the ``foo`` package.

``pkg_resources`` had useful functionality. And it was the recommended
mechanism for loading resource files for several years. But it wasn't
part of the Python standard library and needed to be explicitly installed.
So not everyone used it.

Python 3.1 added the ``importlib`` package, which is the primary home for
all core functionality related to ``import``. Python importers were now
defined via interfaces. One of those interfaces is ``ResourceLoader``. It
has a single method ``get_data(path)``. Given a Python module's ``loader``
(e.g. via the ``__loader__`` attribute on the module), you could call
``get_data(path)`` and load a resource. e.g.
``import foo; foo.__loader__.get_data("bar.txt")``.

The standard library only had ``ResourceLoader`` for several years. And
``ResourceLoader`` wasn't exactly a convenient API to use because it was
so low-level. Many Python applications continued to use ``pkg_resources``
or direct file-based I/O.

Python 3.7 introduced significant improvements to resource loading in
the standard library.

At a low level, module loaders could now implement a
``get_resource_reader(name)`` method, which would return an object
implementing the
`ResourceReader <https://docs.python.org/3.7/library/importlib.html#importlib.abc.ResourceReader>`_
interface. This interface defined methods like ``open_resource(name)``
and ``contents()`` to open a file-like handle on a named resource and
obtain a list of all available resources.

At a high level, the
`importlib.resources <https://docs.python.org/3.7/library/importlib.html#module-importlib.resources>`_
package provided a user-friendly API for interacting with ``ResourceReader``
instances. You could call e.g.
``importlib.resources.open_binary(package, name)`` to obtain a file-like
handle on a specific resource within a package.

Python 3.7's new resource APIs finally gave the Python standard library
access to powerful APIs for loading resources without using a 3rd
party package (like ``pkg_resources``).

At the time of writing this in April 2020, it looks like Python 3.9 will
invent yet another low-level resource loading API.

Because Python hasn't had a robust resource loading API in the standard
library for much of its history, lots of Python code in the wild does
not make use of the APIs in the standard library. It is not uncommon
to see code in 2020 that still uses ``__file__`` to load resources.
Furthermore, because Python 3.7 is still relatively young and code may
wish to maintain compatibility with older Python versions, the newer APIs
may be actively avoided.

.. important::

   As of Python 3.8, ``ResourceReader`` and ``importlib.resources`` are the
   most robust mechanisms for loading resources and we recommend
   adopting these APIs if possible.

.. _resource_reader_support:

Support for ``ResourceReader``
==============================

``oxidized_importer`` implements the ``ResourceReader`` interface for
loading resource files.

However, compatibility with Python's default filesystem-based implementation
can vary. Unfortunately, various behavior with ``ResourceReader`` is
`undefined <https://bugs.python.org/issue36128>`_, so it isn't clear
if CPython or ``oxidized_importer`` is buggy here.

``oxidized_importer`` maintains an index of known resource files.
This index is logically a ``dict`` of ``dict``s, where the outer key is
the Python package name and the inner key is the resource name. Package
names are fully qualified. e.g. ``foo`` or ``foo.bar``. Resource names
are effectively relative filesystem paths. e.g. ``resource.txt`` or
``subdir/resource.txt``. The relative paths always use ``/`` as the
directory separator, even on Windows.

``OxidizedFinder.get_resource_reader()`` returns instances of
``OxidizedResourceReader``. Each instance is bound to a specific Python
package: that's how they are defined. When an ``OxidizedResourceReader``
receives the name of a resource, it performs a simple lookup in the global
resources index. If the string key is found, it is used. Otherwise, it is
assumed the resource doesn't exist.

The ``OxidizedResourceReader.contents()`` method will return a list of all
keys in the internal resources index.

``OxidizedResourceReader`` works the same way for in-memory and
filesystem-relative :ref:`packaging_resource_locations` because internally
both use the same index of resources to drive execution: only the location
of the resource content varies.

``OxidizedResourceReader``'s implementation varies from the standard library
filesystem-based implementation in the following ways:

* ``OxidizedResourceReader.contents()`` will return keys from the package's
  resources dictionary, not all the files in the same directory as the
  underlying Python package (the standard library uses ``os.listdir()``).
  ``OxidizedResourceReader`` will therefore return resource names in
  sub-directories as long as those sub-directories aren't themselves Python
  packages.
* Resources must be explicitly registered with ``OxidizedFinder`` as such in
  order   to be exposed via the resources API. By contrast, the
  filesystem-based   importer - relying on ``os.listdir()`` - will expose
  all files in a directory as a resource. This includes ``.py`` files.
* ``OxidizedResourceReader.is_resource()`` will return ``True`` for resource
  names containing a slash. Contrast with Python's, which returns ``False``
  (even though you can open a resource with ``ResourceReader.open_resource()``
  for the same path). ``OxidizedResourceReader``'s behavior is more
  consistent.

.. _resource_loader_support:

Support for ``ResourceLoader``
==============================

``OxidizedFinder`` implements the deprecated ``ResourceLoader`` interface
and ``get_data(path)`` will return ``bytes`` instances for registered
resources or raise ``OSError`` on request of an unregistered resource.

The path passed to ``get_data(path)`` MUST be an absolute path that has the
prefix of either the currently running executable file or the directory
containing it.

If the resource path is prefixed with the current executable's path, the
path components after the current executable path are interpreted as the
path to a resource registered for in-memory loading.

If the resource path is prefixed with the current executable's directory,
the path components after this directory are interpreted as the path to a
resource registered for application-relative loading.

All other resource paths aren't recognized and an ``OSError`` will be
raised. There is no fallback to loading from the filesystem, even if a
valid filesystem path pointing to an existing file is passed in.

.. note::

   The behavior of not servicing paths that actually exist but aren't
   registered with ``OxidizedFinder`` as resources may be overly opinionated
   and undesirable for some applications.

   If this is a legitimate use case for your application, please create a
   GitHub issue to request this feature.

Once a path is recognized as having the prefix of the current executable
or its directory, the remaining path components will be interpreted as the
resource path. This resource path logically contains a package name component
and a resource name component. ``OxidizedFinder`` will traverse all
potential package names starting from the longest/deepest up until the
top-level package looking for a known Python package. Once a known package
name is encountered, its resources will be consulted. At most 1 package
will be consulted for resources.

Here is a concrete example.

If the ``path`` is ``/usr/bin/myapp/foo/bar/resource.txt`` and the current
executable is ``/usr/bin/myapp``, the requested resource will be
``foo/bar/resource.txt``. Since the path was prefixed with the executable
path, only resources registered for in-memory loading will be consulted.

Our candidate package names are ``foo.bar`` and ``foo``, in that order.

If ``foo.bar`` is a known package and ``resource.txt`` is registered for
in-memory loading, that resource's contents will be returned.

If ``foo.bar`` is a known package and ``resource.txt`` is not registered
in that package, ``OSError`` is raised.

If ``foo.bar`` is not a known package, we proceed to check for package
``foo``.

If ``foo`` is a known package and ``bar/resource.txt`` is registered
for in-memory loading, its contents will be returned.

Otherwise, we're out of possible packages, so ``OSError`` is raised.

Similar logic holds for resources registered for filesystem-relative loading.
The difference here is the stripped path prefix and we are only looking
for resources registered for filesystem-relative loading. Otherwise, the
traversal logic is exactly the same.

If ``OSError`` is raised due to a missing resource, its ``errno`` is ``ENOENT``
and its ``filename`` is the passed in ``path``. Python should automatically
translate this to a ``FileNotFoundError`` exception. But callers should
catch ``OSError``, as other ``OSError`` variants can be raised (e.g. for
file permission errors).

Support for ``__file__``
========================

``OxidizedFinder`` may or may not set the ``__file__`` attribute on loaded
modules. See :ref:`no_file` for details.

Therefore, Python code relying on the presence of ``__file__`` to derive
paths to resource files may or may not work with ``oxidized_importer``.

Code utilizing ``__file__`` for resource loading is highly encouraged to switch
to the ``importlib.resources`` API. If this is not possible, you can change
packaging settings to move the :ref:`packaging_resource_locations` from
in-memory to filesystem-relative, as ``__file__`` is set when loading modules
from the filesystem.

Support for ``pkg_resources``
=============================

``pkg_resources``'s APIs for loading resources likely do not work with
``oxidized_importer``.

Porting Code to Modern Resources APIs
=====================================

Say you have resources next to a Python module. Legacy code *inside a module*
might do something like the following:

.. code-block:: python

   def get_resource(name):
       """Return a file handle on a named resource next to this module."""
       module_dir = os.path.abspath(os.path.dirname(__file__))
       # Warning: there is a path traversal attack possible here if
       # name continues values like ../../../../../etc/password.
       resource_path = os.path.join(module_dir, name)

       return open(resource_path, 'rb')

Modern code targeting Python 3.7+ can use the ``ResourceReader`` API directly:

.. code-block:: python

   def get_resource(name):
       """Return a file handle on a named resource next to this module."""
       # get_resource_reader() may not exist or may return None, which this
       # code doesn't handle.
       reader = __loader__.get_resource_reader(__name__)
       return reader.open_resource(name)

The ``ResourceReader`` interface is quite low-level. If you want something
higher level or want to access resources outside the current module, it
is recommended to use the
`importlib.resources <https://docs.python.org/3.7/library/importlib.html#module-importlib.resources>`_
APIs. e.g.:

.. code-block:: python

   import importlib.resources

   with importlib.resources.open_binary('mypackage', 'resource-name') as fh:
       data = fh.read()

The ``importlib.resources`` functions are glorified wrappers around the
low-level interfaces on module loaders. But they do provide some useful
functionality, such as additional error checking and automatic importing
of modules, making them useful in many scenarios, especially when loading
resources outside the current package/module.

Maintaining Compatibility With Python <3.7
==========================================

If you want to maintain compatibility with Python <3.7, you can't use
``ResourceReader`` or ``importlib.resources``, as they are not available.
The recommended solution here is to use a shim.

The best shim to use is
`importlib_resources <https://importlib-resources.readthedocs.io/en/latest/index.html>`_.
This is a standalone Python package that is a backport of ``importlib.resources``
to older Python versions. Essentially, you can always get the APIs from the
latest Python version. This shim knows about the various APIs available
on ``Loader`` instances and chooses the best available one. It should
*just work* with ``oxidized_importer``.

If you want to implement your own shim without introducing a dependency
on ``importlib_resources``, the following code can be used as a starting
implementation:

.. code-block:: python

   import importlib

   try:
       import importlib.resources
       # Defeat lazy module importers.
       importlib.resources.open_binary
       HAVE_RESOURCE_READER = True
   except ImportError:
       HAVE_RESOURCE_READER = False

   try:
       import pkg_resources
       # Defeat lazy module importers.
       pkg_resources.resource_stream
       HAVE_PKG_RESOURCES = True
   except ImportError:
       HAVE_PKG_RESOURCES = False


   def get_resource(package, resource):
       """Return a file handle on a named resource in a Package."""

       # Prefer ResourceReader APIs, as they are newest.
       if HAVE_RESOURCE_READER:
           # If we're in the context of a module, we could also use
           # ``__loader__.get_resource_reader(__name__).open_resource(resource)``.
           # We use open_binary() because it is simple.
           return importlib.resources.open_binary(package, resource)

       # Fall back to pkg_resources.
       if HAVE_PKG_RESOURCES:
           return pkg_resources.resource_stream(package, resource)

       # Fall back to __file__.

       # We need to first import the package so we can find its location.
       # This could raise an exception!
       mod = importlib.import_module(package)

       # Undefined __file__ will raise NameError on variable access.
       try:
           package_path = os.path.abspath(os.path.dirname(mod.__file__))
       except NameError:
           package_path = None

       if package_path is not None:
           # Warning: there is a path traversal attack possible here if
           # resource contains values like ../../../../etc/password. Input
           # must be trusted or sanitized before blindly opening files or
           # you may have a security vulnerability!
           resource_path = os.path.join(package_path, resource)

           return open(resource_path, 'rb')

       # Could not resolve package path from __file__.
       raise Exception('do not know how to load resource: %s:%s' % (
                       package, resource))

(The above code is dedicated to the public domain and can be used without
attribution.)

This code is provided for example purposes only. It may or may not be sufficient
for your needs.
