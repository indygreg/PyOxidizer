.. _config_files:

===================
Configuration Files
===================

PyOxidizer uses `Starlark <https://github.com/bazelbuild/starlark>`_
files to configure run-time behavior.

Starlark is a dialect of Python intended to be used as a configuration
language and the syntax should be familiar to any Python programmer.

This documentation section contains both a high-level overview of
the configuration files and their semantics as well as low-level
documentation for every type and function in the Starlark dialect.

.. toctree::
   :maxdepth: 3

   config_locating
   config_concepts
   config_resource_add_attributes
   config_globals
   config_global_state
   config_target_management
   config_filesystem
   config_tugger_extensions
   config_type_file
   config_type_python_distribution
   config_type_python_embedded_resources
   config_type_python_executable
   config_type_python_extension_module
   config_type_python_interpreter_config
   config_type_python_module_source
   config_type_python_package_resource
   config_type_python_package_distribution_resource
   config_type_python_packaging_policy
