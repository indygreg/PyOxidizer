.. _pyoxy_overview:

========
Overview
========

The ``pyoxy`` Executable
========================

PyOxy is distributed as a ``pyoxy`` compiled executable. This executable
links against a Python implementation/distribution (i.e. ``libpython``).

The Python implementation/distribution and any resources defined in its standard
library *may* be compiled statically into the ``pyoxy`` executable. This
enables ``pyoxy`` to function as a single file Python distribution. (This is
how official builds of ``pyoxy`` are distributed.)

``pyoxy``'s ``int main()`` is implemented in Rust. It simply parses the process
arguments and executes a sub-command.

Full Python Interpreter Control
===============================

Commands like ``pyoxy run-yaml`` (see :ref:`pyoxy_yaml`) give you very low-level
control over the behavior of the Python interpreter: much lower than what is
possible with ``python`` command arguments or environment variables.

This control can be useful for iterating/testing on different Python embedding
configurations (such as how you would need to configure PyOxidizer). The control
can also be useful for use in automated testing where you may want to simulate
an embedded Python configuration but don't want to produce your own executable
for each configuration variation. With commands like ``pyoxy run-yaml``, you
can simply define a YAML file defining the interpreter configuration and use
a single executable for driving the Python interpreter N ways.

Additional Python Features
==========================

``pyoxy`` supplements the built-in features of ``python`` with its own.

With ``pyoxy``, you can:

* Dynamically choose from the system, jemalloc, mimalloc, or snmalloc memory
  allocators.
* Easily leverage the ``oxidized_importer`` extension module for importing
  Python modules and loading file-based resources faster than the official
  importers in the Python standard library.
* Automatically discover the location of the ``terminfo`` database at runtime,
  helping to ensure terminal functionality works as intended.
* Automatically write a file containing a list of imported modules when the
  Python interpreter finalizes.
* And more.

``pyoxy`` aims to expose all the value-added features implemented in the
``pyembed`` Rust crate via the CLI so Python developers can harness these
features without having to use something more heavyweight, like PyOxidizer.

Masquerading as ``python``
==========================

The ``pyoxy run-python`` command can be used to make the executable behave like
``python`` would. e.g. ``pyoxy run-python -- -c "print('hello, world')"``.

In addition, if the ``pyoxy`` executable's file name begins with ``python``
(e.g. ``python``, ``python3``, ``python3.9``, ``python.exe``), its custom
argument parsing is short-circuited and the executable will behave as if it
is actually ``python``. This theoretically enables ``pyoxy`` to be used as
a drop-in replacement for ``python``.

.. code-block::

   $ mv pyoxy python
   $ ./python
   Python 3.9.5 (default, May 11 2021, 08:20:37)
   [GCC 10.3.0] on linux
   Type "help", "copyright", "credits" or "license" for more information.
   >>>
