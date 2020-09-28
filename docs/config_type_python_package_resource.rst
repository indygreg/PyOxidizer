.. _config_type_python_package_resource:

=========================
``PythonPackageResource``
=========================

This type represents a resource _file_ in a Python package. It is
effectively a named blob associated with a Python package. It is
typically accessed using the ``importlib.resources`` API.

Each instance has the following attributes:

``package`` (string)
   Python package this resource is associated with.

``name`` (string)
   Name of this resource.

``is_stdlib`` (``bool``)
   Whether this module is part of the Python standard library (part of the
   Python distribution).
