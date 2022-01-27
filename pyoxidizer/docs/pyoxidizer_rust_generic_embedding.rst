.. py:currentmodule:: starlark_pyoxidizer

.. _pyoxidizer_rust_generic_embedding:

=============================================
Generic Python Embedding in Rust Applications
=============================================

PyOxidizer can be used to produce artifacts facilitating the embedding
of Python in a Rust application. This enables Rust developers to
leverage PyOxidizer's technology for linking an embedded Python and
managing the Python interpreter at run-time without a build-time
dependency on PyOxidizer. This can greatly simplify development
workflows at the cost of not being able to utilize the full power of
PyOxidizer during builds. If you would like to use PyOxidizer as a
build dependency, see :ref:`rust_projects` instead.

Producing Embedding Artifacts
=============================

The ``pyoxidizer generate-python-embedding-artifacts`` command can be
used to write Python embedding artifacts into an output directory. e.g.::

   $ pyoxidizer generate-python-embedding-artifacts artifacts
   $ ls artifacts
   default_python_config.rs  libpython3.a  packed-resources  pyo3-build-config-file.txt stdlib tcl

This command essentially runs ``pyoxidizer run-build-script`` with a default
configuration file that produces artifacts suitable for generic Python
embedding scenarios.

The Written Artifacts
=====================

``pyoxidizer generate-python-embedding-artifacts`` will write the following
files.

A Linkable Python Library
-------------------------

On UNIX platforms, this will likely be named ``libpython3.a``. On Windows,
``python3.dll`` and a ``pythonXY.dll`` (where ``XY`` is the major-minor Python
version, e.g. ``39``).

The library can be linked to provide an embedded Python interpreter.

A Rust Source File Containing a Python Interpreter Config
---------------------------------------------------------

The ``default_python_config.rs`` file contains the definition of a
``pyembed::OxidizedPythonInterpreterConfig`` Rust struct for defining an
embedded Python interpreter. The config should *just work* with the other
files produced.

You can ``include!(...)`` this file in your Rust program if you want. Or
you can ignore it and write your own configuration.

Packed Resources for the Standard Library
-----------------------------------------

A file containing the :ref:`python_packed_resources` for the Python standard
library will be written. This file can be used by :ref:`oxidized_importer` to
import the standard library efficiently.

PyO3 Build Configuration
------------------------

A ``pyo3-build-config-file.txt`` file will be written defining a configuration
for the ``pyo3-build-config`` crate which will link the ``libpython`` produced
by this command.

To use this configuration, set the ``PYO3_CONFIG_FILE`` environment variable
to its **absolute** path and Python should get linked the way PyOxidizer would
link it.

Python Standard Library
-----------------------

The ``stdlib`` directory will contain a copy of the Python standard library
as it existed in the source distribution.

.. note::

   ``.pyc`` files are often not present and PyOxidizer doesn't yet provide a
   turnkey way to produce these files.

Tcl/tk Support Files
--------------------

The ``tcl`` directory will contain tcl/tk support files to support the
``tkinter`` Python module.

Example Workflows
=================

Embed Python With ``pyo3``
--------------------------

In this example, we will produce a Rust executable that uses the ``pyo3``
crate for interfacing with an embedded Python interpreter. We will not use
PyOxidizer's ``pyembed`` crate or the ``oxidized_importer`` extension module
for enhancing functionality of Python.

First, create a new Rust project::

   $ cargo init --bin pyapp

Then edit its ``Cargo.toml`` to add the ``pyo3`` dependency. e.g.

.. code-block:: toml

   [package]
   name = "pyapp"
   version = "0.1.0"
   edition = "2021"

   [dependencies]
   pyo3 = "0.14"

And define a ``src/main.rs``:

.. code-block:: rust

   use pyo3::prelude::*;

   fn main() -> PyResult<()> {
       unsafe {
           pyo3::with_embedded_python_interpreter(|py| {
               py.run("print('hello, world')", None, None)
           })
       }
   }

Now use ``pyoxidizer`` to generate the Python embedding artifacts::

   $ pyoxidizer generate-python-embedding-artifacts pyembedded

And finally build the Rust project using the PyO3 configuration file to
tell PyO3 how to link the Python library we just generated::

   $ PYO3_CONFIG_FILE=$(pwd)/pyembedded/pyo3-build-config-file.txt cargo run

If you are doing this on a UNIX-like platform like Linux or macOS, chances are
this fails with an error similar to the following::

    Could not find platform independent libraries <prefix>
    Could not find platform dependent libraries <exec_prefix>
    Consider setting $PYTHONHOME to <prefix>[:<exec_prefix>]
    Python path configuration:
      PYTHONHOME = (not set)
      PYTHONPATH = (not set)
      program name = 'python3'
      isolated = 0
      environment = 1
      user site = 1
      import site = 1
      sys._base_executable = '/usr/bin/python3'
      sys.base_prefix = '/install'
      sys.base_exec_prefix = '/install'
      sys.platlibdir = 'lib'
      sys.executable = '/usr/bin/python3'
      sys.prefix = '/install'
      sys.exec_prefix = '/install'
      sys.path = [
        '/install/lib/python39.zip',
        '/install/lib/python3.9',
        '/install/lib/lib-dynload',
      ]
    Fatal Python error: init_fs_encoding: failed to get the Python codec of the filesystem encoding
    Python runtime state: core initialized
    ModuleNotFoundError: No module named 'encodings'

    Current thread 0x00007ffa5abd9c80 (most recent call first):
    <no Python frame>

This is because the embedded Python library doesn't know how to locate the
Python standard library. Essentially, the compiled Python library has some
hard-coded defaults for where the Python standard library is located and its
default logic is to search in those paths. The references to ``/install`` are
referring to the build environment for the Python distributions.

The quick fix for this is to define the ``PYTHONPATH`` environment variable to
the location of the Python standard library. e.g.::

   $ PYO3_CONFIG_FILE=$(pwd)/pyembedded/pyo3-build-config-file.txt PYTHONPATH=pyembedded/stdlib cargo run
   Could not find platform independent libraries <prefix>
   Could not find platform dependent libraries <exec_prefix>
   Consider setting $PYTHONHOME to <prefix>[:<exec_prefix>]
   hello, world

We still get some warnings. But our embedded Python interpreter does work!

To make these config changes more permanent and to silence the remaining
warnings, you'll need to customize the initialization of the Python interpreter
using C APIs like the
`Python Initialization Configuration <https://docs.python.org/3/c-api/init_config.html>`_
APIs. This requires a fair bit of ``unsafe`` code.

Abstracting away the complexities of initializing the embedded Python
interpreter is one of the reasons the :ref:`pyembed <pyembed>` Rust crate
exists. So if you want a simpler approach, consider using ``pyembed`` for
controlling the Python interpreter.

Embed Python with ``pyembed``
-----------------------------

In this example we'll use the :ref:`pyembed <pyembed>` crate (part of the
PyOxidizer project) for managing the embedded Python interpreter.

First, create a new Rust project::

   $ cargo init --bin pyapp

Then edit its ``Cargo.toml`` to add the ``pyembed`` dependency. e.g.

.. code-block:: toml

   [package]
   name = "pyapp"
   version = "0.1.0"
   edition = "2021"

   [dependencies]
   # Check for the latest version in case these docs are out of date.
   pyembed = "0.18"

And define a ``src/main.rs``:

.. code-block:: rust

    include!("../pyembedded/default_python_config.rs");

    fn main() {
        // Get config from default_python_config.rs.
        let config = default_python_config();

        let interp = pyembed::MainPythonInterpreter::new(config).unwrap();

        // `py` is a `pyo3::Python` instance.
        interp.with_gil(|py| {
            py.run("print('hello, world')", None, None).unwrap();
        });

    }

Now use ``pyoxidizer`` to generate the Python embedding artifacts::

   $ pyoxidizer generate-python-embedding-artifacts pyembedded

And finally build the Rust project using the PyO3 configuration file to
tell PyO3 how to link the Python library we just generated::

   $ PYO3_CONFIG_FILE=$(pwd)/pyembedded/pyo3-build-config-file.txt cargo run
   ...
    Finished dev [unoptimized + debuginfo] target(s) in 3.87s
     Running `target/debug/pyapp`
   hello, world

If all goes as expected, this should *just work*!
