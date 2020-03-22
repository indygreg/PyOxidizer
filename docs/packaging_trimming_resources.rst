.. _packaging_trimming_resources:

=========================
Trimming Unused Resources
=========================

By default, packaging rules are very aggressive about pulling in
resources such as Python modules. For example, the entire Python standard
library is embedded into the binary by default. These extra resources take up
space and can make your binary significantly larger than it could be.

It is often desirable to *prune* your application of unused resources. For
example, you may wish to only include Python modules that your application
uses. This is possible with ``PyOxidizer``.

Essentially, all strategies for managing the set of packaged resources
boil down to crafting config file logic that chooses which resources
are packaged.

But maintaining explicit lists of resources can be tedious. ``PyOxidizer``
offers a more automated approach to solving this problem.

The :ref:`config_python_interpreter_config` type defines a
``write_modules_directory_env`` setting, which when enabled will instruct
the embedded Python interpreter to write the list of all loaded modules
into a randomly named file in the directory identified by the environment
variable defined by this setting. For example, if you set
``write_modules_directory_env="PYOXIDIZER_MODULES_DIR"`` and then
run your binary with ``PYOXIDIZER_MODULES_DIR=~/tmp/dump-modules``,
each invocation will write a ``~/tmp/dump-modules/modules-*`` file
containing the list of Python modules loaded by the Python interpreter.

One can therefore use ``write_modules_directory_env`` to produce files
that can be referenced in a different build *target* to filter resources
through a set of *only include* names.

TODO this functionality was temporarily dropped as part of the Starlark
port.
