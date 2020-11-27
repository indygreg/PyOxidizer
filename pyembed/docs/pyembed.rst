.. _pyembed:

The ``pyembed`` Rust Crate
==========================

The ``pyembed`` Rust crate facilitates the embedding of a Python interpreter
in a Rust binary.

The crate provides an API for instantiating and controlling an embedded
Python interpreter. It also defines a custom *meta path importer* that can
be used to import Python resources (such as module bytecode) from memory.

.. toctree::
   :maxdepth: 2

   pyembed_crate_configuration
   pyembed_controlling_python
   pyembed_extension_modules
