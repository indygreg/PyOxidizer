.. py:currentmodule:: starlark_pyoxidizer

.. _rust_rust_code:

=================================
Controlling Python From Rust Code
=================================

PyOxidizer can be used to embed Python in a Rust application.

This page documents what that looks like from a Rust code perspective.

Interacting with the ``pyembed`` Crate
======================================

When writing Rust code to interact with a Python interpreter, your
primary area of contact will be with the ``pyembed`` crate.

The ``pyembed`` crate is a standalone crate maintained as part of the
PyOxidizer project. This crate provides the core run-time functionality
for PyOxidizer, such as the implementation of
:ref:`PyOxidizer's custom importer <oxidized_importer>`. It also exposes
a high-level API for initializing a Python interpreter and running code
in it.

See :ref:`pyembed` for full documentation on the ``pyembed`` crate.
:ref:`pyembed_controlling_python` in particular describes how to interface
with the embedded Python interpreter.

The following documentation will be unique to PyOxidizer's use of the
``pyembed`` crate.

Using the Default ``OxidizedPythonInterpreterConfig``
=====================================================

When using a PyOxidizer-generated Rust project and that project is configured
to use PyOxidizer to build (the default), that project/crate's build script
will call into PyOxidizer to emit various build artifacts. This will process
the PyOxidizer configuration file and write some files somewhere.

One of the files generated is a Rust source file containing a
``fn default_python_config() -> pyembed::OxidizedPythonInterpreterConfig`` which
emits a ``pyembed::OxidizedPythonInterpreterConfig`` using the configuration
from the PyOxidizer configuration file. This configuration is based off the
:py:class:`PythonInterpreterConfig` defined in the PyOxidizer Starlark
configuration file.

The crate's build script will set the ``DEFAULT_PYTHON_CONFIG_RS``
environment variable to the path to this file, exposing it to Rust code.

This all means that to use the auto-generated
``pyembed::OxidizedPythonInterpreterConfig`` instance with your Rust application,
you simply need to do something like the following:

.. code-block:: rust

   include!(env!("DEFAULT_PYTHON_CONFIG_RS"));

   fn create_interpreter() -> Result<pyembed::MainPythonInterpreter> {
       // Calls function from include!()'d file.
       let config: pyembed::OxidizedPythonInterpreterConfig = default_python_config();

       pyembed::MainPythonInterpreter::new(config)
   }

Using a Custom ``OxidizedPythonInterpreterConfig``
--------------------------------------------------

If you don't want to use the default
``pyembed::OxidizedPythonInterpreterConfig`` instance, that's fine too! However,
this will be slightly more complicated.

First, if you use an explicit ``OxidizedPythonInterpreterConfig``, the
:py:class:`PythonInterpreterConfig` Starlark
type defined in your PyOxidizer configuration file doesn't matter that much.
The primary purpose of this Starlark type is to derive the default
``OxidizedPythonInterpreterConfig`` Rust struct. And if you are using your own
custom ``OxidizedPythonInterpreterConfig`` instance, you can ignore most of the
arguments when creating the ``PythonInterpreterConfig`` instance.

An exception to this is the ``raw_allocator`` argument/field. If you
are using a custom allocator (like jemalloc, mimalloc, or snmalloc), you will need
to enable a Cargo feature when building the ``pyembed`` crate or else you will get
a run-time error that the specified allocator is not available.

``pyembed::OxidizedPythonInterpreterConfig::default()`` can be used to
construct a new instance, pre-populated with default values for each field.
The defaults should match what the :py:class:`PythonInterpreterConfig`
Starlark type would yield.

The main catch to constructing the instance manually is that the custom
*meta path importer* won't be able to service Python ``import`` requests
unless you populate a few fields. In fact, if you just use the defaults,
things will blow up pretty hard at run-time::

   $ myapp
   Fatal Python error: initfsencoding: Unable to get the locale encoding
   ModuleNotFoundError: No module named 'encodings'

   Current thread 0x00007fa0e2cbe9c0 (most recent call first):
   Aborted (core dumped)

What's happening here is that Python interpreter initialization hits a fatal
error because it can't ``import encodings`` (because it can't locate the
Python standard library) and Python's C code is exiting the process. Rust
doesn't even get the chance to handle the error, which is why we're seeing
a segfault.

The reason we can't ``import encodings`` is twofold:

1. The default filesystem importer is disabled by default.
2. No Python resources are being registered with the
   ``OxidizedPythonInterpreterConfig`` instance.

This error can be addressed by working around either.

To enable the default filesystem importer:

.. code-block:: rust

   let mut config = pyembed::OxidizedPythonInterpreterConfig::default();
   config.filesystem_importer = true;
   config.sys_paths.push("/path/to/python/standard/library");

As long as the default filesystem importer is enabled and ``sys.path``
can find the Python standard library, you should be able to
start a Python interpreter.

.. hint::

   The ``sys_paths`` field will expand the special token ``$ORIGIN`` to the
   directory of the running executable. So if the Python standard library is
   in e.g. the ``lib`` directory next to the executable, you can do something
   like ``config.sys_paths.push("$ORIGIN/lib")``.

If you want to use the custom :ref:`PyOxidizer Importer <oxidized_importer>`
to import Python resources, you will need to update a handful of fields:

.. code-block:: rust

   let mut config = pyembed::OxidizedPythonInterpreterConfig::default();
   config.packed_resources = ...;
   config.oxidized_importer = true;

The ``packed_resources`` field defines a reference to *packed resources
data* (a ``PackedResourcesSource`` enum. This is a custom serialization
format for expressing *resources* to make available to a Python interpreter. See
:ref:`python_packed_resources` for more. The easiest way to obtain this
data blob is by using PyOxidizer and consuming the ``packed-resources``
build artifact/file, likely though ``include_bytes!``.
:ref:`oxidized_finder` can also be used to produce these data structures.

Finally, setting ``oxidized_importer = true`` is necessary to enable
:py:class:`oxidized_importer.OxidizedFinder`.
