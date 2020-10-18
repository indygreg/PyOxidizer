.. _config_type_file:

========
``File``
========

This type represents a concrete file in an abstract filesystem. The
file has a path and content.

Instances can be constructed by calling methods that emit resources
with a :ref:`config_type_python_packaging_policy` having
:ref:`config_type_python_packaging_policy_file_scanner_emit_files`
set to ``True``.

Attributes
==========

The following sections describe the attributes available on each
instance.

.. _config_type_file_path:

``path``
--------

(``string``)

The filesystem path represented. Typically relative. Doesn't
have to correspond to a valid, existing file on the filesystem.

``is_executable``
-----------------

(``bool``)

Whether the file is executable.

``add_*``
---------

(various)

See :ref:`config_resource_add_attributes`.
