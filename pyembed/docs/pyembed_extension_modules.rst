.. _pyembed_extension_modules:

====================================
Adding Extension Modules At Run-Time
====================================

A Python extension module is effectively a callable function defined in
a library somewhere.

The ``pyembed`` crate supports registering Python extension modules
multiple ways.

Statically Linked Extension Modules
===================================

You can inform the ``pyembed`` crate about the existence of additional
Python extension modules which are statically linked into the binary.

To do this, you will need to populate the ``extra_extension_modules`` field
of the ``OxidizedPythonInterpreterConfig`` Rust struct used to construct the
Python interpreter. Simply add an entry defining the extension module's
``import`` name and a pointer to its C initialization function
(often named ``PyInit_<name>``. e.g. if you are defining the extension
module ``foo``, the initialization function would be ``PyInit_foo``
by convention.

Please note that Python stores extension modules in a global variable.
So instantiating multiple interpreters via the ``pyembed`` interfaces may
result in duplicate entries or unwanted extension modules being exposed to
the Python interpreter.

Dynamically Linked Extension Modules
====================================

If you have an extension module provided as a shared library (this is typically
how Python extension modules work), it will be possible to load this
extension module provided that the Python interpreter supports loading
dynamically linked Python extension modules.

There is not yet an explicit Rust API for loading additional dynamically
linked extension modules. It is theoretically possible to add an entry
to the parsed embedded resources data structure. The path of least resistance
is likely to enable the standard filesystem importer and put your shared
library extension module somewhere on Python's ``sys.path``. (This is how
extension modules are typically loaded.)
