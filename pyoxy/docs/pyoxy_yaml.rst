.. _pyoxy_yaml:

===============================
Running YAML Based Applications
===============================

The ``pyoxy run-yaml`` command enables you to run a Python interpreter given
a configuration defined in a YAML file.

Usage
=====

Run ``pyoxy help run-yaml`` to see full documentation.

The high-level operation is::

   pyoxy run-yaml [FILE] [-- <args>...]

You give the command the path to a YAML file to parse and optional additional
arguments following a ``--``. e.g.::

   pyoxy run-yaml myapp.yaml
   pyoxy run-yaml myapp.yaml -- --arg true

File Parsing
============

We use a customized mechanism for parsing the specified content for a YAML
config. The rules are as follows:

* The file MUST be UTF-8. (YAML allows encodings other than UTF-8. We do not yet
  support alternative encodings, such as UTF-16.)
* The content of the file up to a line beginning with ``---`` is ignored.
* Parsing stops when a line beginning with ``...`` is encountered.
* All content between the initial line beginning with ``---`` and either a) the
  first line beginning with ``...`` or b) the end of the file is parsed as YAML.

YAML Configuration
==================

The YAML document attempts to deserialize to a ``pyembed::OxidizedPythonInterpreterConfig``
Rust struct. This type and its fields are extensively documented at
:ref:`pyoxy_struct_OxidizedPythonInterpreterConfig`.

Some of the most important fields in the configuration data structure define
what to run when the interpreter starts. e.g.

.. code-block:: yaml

   ---
   interpreter_config:
     run_command: 'print("hello, world")'
   ...

.. code-block:: yaml

   ---
   interpreter_config:
     run_module: 'mypackage.__main__'
   ...

Portable Invocation Using a Shell Shebang
=========================================

On UNIX-like platforms, files containing an embedded YAML config can be
made to execute with ``pyoxy run-yaml`` by using a specially crafted shebang
(leading ``#!`` line) and making the file executable.

For example, say you distribute the ``pyoxy`` binary in the same directory
as your executable ``myapp`` file. Here's what ``myapp`` would look like:

.. code-block::

   #!/bin/sh
   "exec" "`dirname $0`/pyoxy" run-yaml "$0" -- "$@"
   ---
   # YAML configuration.
   ...

This file defines a shell script which simply calls ``exec`` to invoke
``pyoxy run-yaml``, giving it the path to the current file and additional
arguments passed to the original invocation. Because our custom YAML parsing
ignores content up to the first line beginning with ``---``, the shebang
and shell script content is ignored and the file evaluates as if those initial
lines did not exist.
