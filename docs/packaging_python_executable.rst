.. _packaging_python_executable:

===================================================
Building an Executable that Behaves Like ``python``
===================================================

It is possible to use PyOxidizer to build an executable that would
behave like a typical ``python`` executable would.

To start, initialize a new config file::

   $ pyoxidizer init-config-file python

Then, we'll want to modify the ``pyoxidizer.bzl`` configuration
file to look something like the following:

.. code-block:: python

   def make_dist():
       return default_python_distribution()

   def make_exe(dist):
       policy = dist.make_python_packaging_policy()
       policy.extension_module_filter = "all"
       policy.include_distribution_resources = True

       # Add resources to the filesytem, next to the built executable.
       # You can add resources to memory too. But this makes the install
       # layout somewhat consistent with what Python expects.
       policy.resources_location = "filesystem-relative:lib"

       python_config = dist.make_python_interpreter_config()

       # This is the all-important line to make the embedded Python interpreter
       # behave like `python`.
       python_config.config_profile = "python"

       # Enable the stdlib path-based importer.
       python_config.filesystem_importer = True

       # You could also disable the Rust importer if you really want your
       # executable to behave like `python`.
       # python_config.oxidized_importer = False

       exe = dist.to_python_executable(
           name="python3",
           packaging_policy = policy,
           config = python_config,
       )

       return exe

   def make_embedded_resources(exe):
       return exe.to_embedded_resources()

   def make_install(exe):
       files = FileManifest()
       files.add_python_resource(".", exe)

       return files

   register_target("dist", make_dist)
   register_target("exe", make_exe, depends=["dist"])
   register_target("resources", make_embedded_resources, depends=["exe"], default_build_script=True)
   register_target("install", make_install, depends=["exe"], default=True)

   resolve_targets()

(The above code is dedicated to the public domain and can be used without
attribution.)

From there, build/run from the config::

   $ cd python
   $ pyoxidizer build
   ...
   $ pyoxidizer run
   ...
   Python 3.8.6 (default, Oct  3 2020, 20:48:20)
   [Clang 10.0.1 ] on linux
   Type "help", "copyright", "credits" or "license" for more information.
   >>>


.. _packaging_python_executable_resource_loading_caveats:

Resource Loading Caveats
========================

PyOxidizer's configuration defaults are opinionated about how resources
are loaded by default. In the default configuration, the Python distribution's
resources are indexed and loaded via ``oxidized_importer`` at run-time.
This behavior is obviously different from what a standard ``python`` executable
would do.

If you want the built executable to behave like ``python`` would and use the
standard library importers, you can disable ``oxidized_importer`` by setting
:ref:`config_type_python_interpreter_config_oxidized_importer` to ``False``.

Another caveat is that indexed resources are embedded in the built executable
by default. This will bloat the size of the executable for no benefit. To
disable this functionality, set
:ref:`config_type_python_executable.packed_resources_load_mode` to ``none``.

Binary Portability
==================

A ``python``-like executable built with PyOxidizer may not *just work*
when copied to another machine. See
:ref:`pyoxidizer_distributing_binary_portability`
to learn more about the portability of binaries built with PyOxidizer.
