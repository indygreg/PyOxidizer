.. _tugger_starlark:

=======================
Tugger Starlark Dialect
=======================

Tugger uses `Starlark <https://github.com/bazelbuild/starlark>`_
files to configure run-time behavior.

Starlark is a subset of Python intended to be used as a configuration
language and the syntax should be familiar to any Python programmer.

Tugger defines its own *dialect* of Starlark - types and functions -
specific to Tugger.

.. toctree::
   :maxdepth: 3

   tugger_starlark_globals
   tugger_starlark_type_file_content
   tugger_starlark_type_file_manifest
