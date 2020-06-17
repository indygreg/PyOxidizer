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

Under the hood, the ``pyembed`` crate uses the ``cpython`` and
``python3-sys`` crates for interacting with Python's C APIs. ``pyembed``
exposes the ``Python`` object from ``cpython``, which means that
once you've initialized a Python interpreter with ``pyembed``, you can
use all the functionality in ``cpython`` to interact with that
interpreter.

Initializing a Python Interpreter
=================================

Initializing an embedded Python interpreter in your Rust process is as simple
as calling ``pyembed::MainPythonInterpreter::new(config: PythonConfig)``.

The hardest part about this is constructing the ``pyembed::PythonConfig``
instance.

Using the Default ``PythonConfig``
----------------------------------

If the ``pyembed`` crate is configured to emit build artifacts (the default),
its build script will generate a Rust source file containing a
``fn default_python_config() -> pyembed::PythonConfig`` which emits a
``pyembed::PythonConfig`` using the configuration as defined by the utilized
PyOxidizer :ref:`configuration file <config_files>`. Assuming you are using the
boilerplate ``Cargo.toml`` and ``build.rs`` script generated with
``pyoxidizer init-rust-project``, the path to this generated source file will
be in the ``PYOXIDIZER_DEFAULT_PYTHON_CONFIG_RS`` environment variable.

This all means that to use the auto-generated ``pyembed::PythonConfig``
instance with your Rust application, you simply need to do something like
the following:

.. code-block:: rust

   include!(env!("PYOXIDIZER_DEFAULT_PYTHON_CONFIG_RS"));

   fn create_interpreter() -> Result<pyembed::MainPythonInterpreter> {
       // Calls function from include!()'d file.
       let config: pyembed::PythonConfig = default_python_config();

       pyembed::MainPythonInterpreter::new(config)
   }

Using a Custom ``PythonConfig``
-------------------------------

If you don't want to use the default ``pyembed::PythonConfig`` instance,
that's fine too! However, this will be slightly more complicated.

First, if you use an explicit ``PythonConfig``, the
:ref:`PythonInterpreterConfig <config_python_interpreter_config>` Starlark
type defined in your PyOxidizer configuration file doesn't matter that much.
The primary purpose of this Starlark type is to derive the default
``PythonConfig`` Rust struct. And if you are using your own custom
``PythonConfig`` instance, you can ignore most of the arguments when
creating the ``PythonInterpreterConfig`` instance.

An exception to this is the ``raw_allocator`` argument/field. If you
are using jemalloc, you will need to enable a Cargo feature when building
the ``pyembed`` crate or else you will get a run-time error that jemalloc
is not available.

``pyembed::PythonConfig::default()`` can be used to construct a new instance,
pre-populated with default values for each field. The defaults should match
what the
:ref:`PythonInterpreterConfig <config_python_interpreter_config>` Starlark
type would yield.

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
2. No Python resources are being registered with the ``PythonConfig``
   instance.

This error can be addressed by working around either.

To enable the default filesystem importer:

.. code-block:: rust

   let mut config = pyembed::PythonConfig::default();
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

   let mut config = pyembed::PythonConfig::default();
   config.packed_resources = ...;
   config.use_custom_importlib = true;

The ``packed_resources`` field defines a reference to *packed resources
data* (a ``&[u8]``. This is a custom serialization format for expressing
*resources* to make available to a Python interpreter. See the
``python-packed-resources`` crate for the format specification and
code for serializing it. Again, the easiest way to obtain this data
blob is by using PyOxidizer and consuming the ``packed-resources``
build artifact/file, likely though ``include_bytes!``.

Finally, setting ``use_custom_importlib = true`` is necessary to enable
the custom bytecode and meta path importer to be used at run-time.

Using a Python Interpreter
==========================

Once you've constructed a ``pyembed::MainPythonInterpreter`` instance, you
can obtain a ``cpython::Python`` instance via ``.acquire_gil()`` and then
use it:

.. code-block:: rust

   fn do_it(interpreter: &MainPythonInterpreter) -> {
       let py = interpreter.acquire_gil().unwrap();

       match pyembed::run_code(py, "print('hello, world')") {
           Ok(_) => print("python code executed successfully"),
           Err(e) => print("python error: {:?}", e),
       }
   }

The ``pyembed`` crate exports various ``run_*`` functions for
performing high-level evaluation of various primitives (files, modules,
code strings, etc). See the ``pyembed`` crate's documentation for more.

Since CPython's API relies on static variables (sadly), if you really wanted
to, you could call out to CPython C APIs directly (probably via the
bindings in the ``python3-sys`` crate) and they would interact with the
interpreter started by the ``pyembed`` crate. This is all ``unsafe``, of course,
so tread at your own peril.

Finalizing the Interpreter
==========================

``pyembed::MainPythonInterpreter`` implements ``Drop`` and it will call
``Py_FinalizeEx()`` when called. So to terminate the Python interpreter, simply
have the ``MainPythonInterpreter`` instance go out of scope or drop it
explicitly.

A Note on the ``pyembed`` APIs
==============================

The ``pyembed`` crate is highly tailored towards PyOxidizer's default use
cases and the APIs are not considered extremely well polished.

While the functionality should work, the ergonomics may not be great.

It is a goal of the PyOxidizer project to support Rust programmers who want
to embed Python in Rust applications. So contributions to improve the quality
of the ``pyembed`` crate will likely be greatly appreciated!
