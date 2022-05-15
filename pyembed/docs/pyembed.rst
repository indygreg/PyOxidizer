.. py:currentmodule:: oxidized_importer

.. _pyembed:

The ``pyembed`` Rust Crate
==========================

The ``pyembed`` Rust crate facilitates the control of an embedded Python
interpreter.

The crate provides an API for instantiating and controlling an embedded
Python interpreter. It also defines a custom *meta path importer* that can
be used to import Python resources (such as module bytecode) from memory.

The crate is developed alongside the PyOxidizer project. However, it is a
generic crate and can be used outside the context of PyOxidizer.

The ``pyembed`` crate is
`published to crates.io <https://crates.io/crates/pyembed>`_ and its Rust
documentation is available at https://docs.rs/pyembed.

.. toctree::
   :maxdepth: 2

   pyembed_building
   pyembed_controlling_python
   pyembed_extension_modules
   pyembed_interpreter_config
