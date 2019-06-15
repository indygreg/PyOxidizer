.. _pitfalls:

==================
Packaging Pitfalls
==================

While PyOxidizer is capable of building fully self-contained binaries
containing a Python application, many Python packages and applications make
assumptions that don't hold inside PyOxidizer. This section talks about
all the things that can go wrong when attempting to package a Python
application.

Reliance on ``__file__``
========================

Python modules typically have a ``__file__`` attribute that defines the
path of the file from which the module was loaded. (When a file is executed
as a script, it masquerades as the ``__main__`` module, so non-module
scripts can behave as modules too.)

It is relatively common for Python modules in the wild to use ``__file__``.
For example, modules may do something like
``module_dir = os.path.abspath(os.path.dirname(__file__))`` to locate the
directory that a module is in so they can load a non-Python file from that
directory. Or they may use ``__file__`` to resolve paths to Python source
files so that they can be loaded outside the typical ``import`` based
mechanism (various plugin systems do this, for example).

Strictly speaking, the ``__file__`` attribute on modules is not required.
Therefore any Python code that requires the existence of ``__file__`` is either
broken or has made an explicit choice to not support module loaders - like
PyOxidizer - that don't store modules as files and may not set ``__file__``.
**Therefore required use of __file__ is highly discouraged.** It is
recommended to instead use a *resources* API for loading *resource* data
relative to a Python module and to fall back to ``__file__`` if a suitable
API is unavailable or doesn't work. See the next section for more.

Resource Reading
================

Many Python application need to load *resources*. *Resources* are typically
non-Python *support* files, such as images, config files, etc. In some cases,
*resources* could be Python source or bytecode files. For example, many
plugin systems load Python modules outside the context of the normal
``import`` mechanism and therefore treat standalone Python source/bytecode
files as non-module *resources*.

PyOxidizer can break existing resource reading code by invalidating assumptions
about where resources are located. Historically, resources almost always
translate to individual paths on the filesystem. One can use ``__file__``
to derive the path to a resource file and ``open()`` the file. So there is
a lot of code in the wild that relies on ``__file__`` for this use case.

.. important::

   Use of ``__file__`` will not work for in-memory resources in PyOxidizer
   applications and Python code will need to use a resource reading API to
   access resources data within the binary.

Depending on your need to support Python versions older than 3.7, the solution
may or may not be simple. That's because for most of its lifetime, Python
hasn't had a robust story for loading *resource* data. ``pkg_resources`` was
the recommended solution for a while. Python 3 introduced the
`ResourceLoader <https://docs.python.org/3.7/library/importlib.html#importlib.abc.ResourceLoader>`_
interface on module loaders. But this interface became deprecated in
Python 3.7 in favor of the
`ResourceReader <https://docs.python.org/3/library/importlib.html#importlib.abc.ResourceReader>`_
interface and associated APIs in the
`importlib.resources module <https://docs.python.org/3/library/importlib.html#module-importlib.resources>`_
But even the modern ``ResourceReader`` interface isn't perfect, as some of its
behavior is `seemingly inconsistent <https://bugs.python.org/issue36128>`_.

``ResourceReader`` is the best interface for importing non-module
*resource* data to date. Unfortunately, that API requires Python 3.7.
And a lot of the Python universe hasn't yet fully adopted Python 3.7 and its
APIs. This means that Python projects in the wild tend to target the
*lowest common denominator* for loading *resource* data. And this solution
tends to be to rely on ``__file__`` (directly or abstracted away) for deriving
paths to things because ``__file__`` has worked nearly everywhere for seemingly
forever.

.. important::

   PyOxidizer supports the
   `ResourceReader <https://docs.python.org/3/library/importlib.html#importlib.abc.ResourceReader>`_
   interface on module loaders and highly encourages Python libraries and
   applications to adopt it as the preferred mechanism for loading resources
   data.

Let's talk about what this means in practice.

Say you have resources next to a Python module. Legacy code in a module
might do something like the following:

.. code-block:: python

   def get_resource(name):
       """Return a file handle on a named resource next to this module."""
       module_dir = os.path.abspath(os.path.dirname(__file__))
       # Warning: there is a path traversal attack possible here if
       # name continues values like ../../../../../etc/password.
       resource_path = os.path.join(module_dir, name)

       return open(resource_path, 'rb')

Modern code targeting Python 3.7+ can use the `ResourceReader` API directly:

.. code-block:: python

   def get_resource(name):
       """Return a file handle on a named resource next to this module."""
       # get_resource_reader() may not exist or may return None, which this
       # code doesn't handle.
       reader = __loader__.get_resource_reader(__name__)
       return reader.open_resource(name)

Alternatively, you can use the functions in
`importlib.resources <https://docs.python.org/3.7/library/importlib.html#module-importlib.resources>`_:

.. code-block:: python

   import importlib.resources

   with importlib.resources.open_binary('mypackage', 'resource-name') as fh:
       data = fh.read()

The ``importlib.resources`` functions are glorified wrappers around the
low-level interfaces on module loaders. But they do provide some useful
functionality, such as additional error checking and automatic importing
of modules, making them useful in many scenarios, especially when loading
resources outside the current package/module.

See the
`importlib_resources documentation site <https://importlib-resources.readthedocs.io/en/latest/index.html>`_
for more.

``ResourceReader`` and ``importlib.resources`` were introduced in Python 3.7.
So if you want your code to remain compatible with older Python versions, you
will need to write an abstraction for obtaining resources. Try something like
the following:

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

The above code is just a demonstration. It may *just work* for your needs.
It may need additional tweaking.

The state of resource management in Python has historically been a mess. So
don't be surprised if you need to modify code to support the modern resource
interfaces. But this effort should be well spent, as the new resource APIs
are hopefully the most future compatible. And, using them will enable
applications built with PyOxidizer to import resources data from memory!
