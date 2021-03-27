.. _config_type_python_embedded_resources:

===========================
``PythonEmbeddedResources``
===========================

The ``PythonEmbeddedResources`` type represents resources made available to
a Python interpreter. The resources tracked by this type are consumed by the
``pyembed`` crate at build and run time. The tracked resources include:

* Python module source and bytecode
* Python package resources
* Shared library dependencies

While the type's name has *embedded* in it, resources referred to by this
type may or may not actually be *embedded* in a Python binary or loaded
directly from the binary. Rather, the term *embedded* comes from the fact
that the data structure describing the resources is typically *embedded*
in the binary or made available to an *embedded* Python interpreter.

Instances of this type are constructed by transforming a type representing
a Python binary. e.g. :ref:`config_python_executable_to_embedded_resources`.

If this type is returned by a target function, its build action will write
out files that represent the various resources encapsulated by this type. There
is no run action associated with this type.
