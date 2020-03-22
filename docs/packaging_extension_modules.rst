.. _packaging_extension_modules:

Adding Extension Modules At Run-Time
====================================

Normally, Python extension modules are compiled into the binary as part
of the embedded Python interpreter or embedded Python resources data
structure.

``PyOxidizer`` also supports providing additional extension modules at run-time.
This can be useful for larger Rust applications providing extension modules
that are implemented in Rust and aren't built through normal Python
build systems (like ``setup.py``).

If the ``PythonConfig`` Rust struct used to construct an embedded Python
interpreter contains a populated ``extra_extension_modules`` field, the
extension modules listed therein will be made available to the Python
interpreter.

Please note that Python stores extension modules in a global variable.
So instantiating multiple interpreters via the ``pyembed`` interfaces may
result in duplicate entries or unwanted extension modules being exposed to
the Python interpreter.
