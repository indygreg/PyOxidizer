.. py:currentmodule:: oxidized_importer

.. _pyembed_controlling_python:

=================================
Controlling Python from Rust Code
=================================

Initializing a Python Interpreter
=================================

Initializing an embedded Python interpreter in your Rust process is as simple
as calling
``pyembed::MainPythonInterpreter::new(config: OxidizedPythonInterpreterConfig)``.

The hardest part about this is constructing the
``pyembed::OxidizedPythonInterpreterConfig`` instance.

Using a Python Interpreter
==========================

Once you've constructed a ``pyembed::MainPythonInterpreter`` instance, you
can obtain a ``pyo3::Python`` instance via ``.with_gil()`` and then
use it:

.. code-block:: rust

   fn do_it(interpreter: &MainPythonInterpreter) -> {
       interpreter.with_gil(|py| {
            match py.eval("print('hello, world')") {
               Ok(_) => print("python code executed successfully"),
               Err(e) => print("python error: {:?}", e),
           }
       });

   }

Since CPython's API relies on static variables (sadly), if you really wanted
to, you could call out to CPython C APIs directly (probably via the
bindings in the ``pyo3`` crate) and they would interact with the
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
