.. _packaging_resources_data:

================================
Managing *Packed* Resources Data
================================

PyOxidizer's custom module importer (see :ref:`oxidized_finder`) reads
data in a custom serialization format (see :ref:`python_packed_resources`)
to facilitate efficient module importing and resource loading. If you
are using this module importer (controlled from the
:ref:`config_type_python_interpreter_config_oxidized_importer` attribute,
which is enabled by default), the interpreter will need to reference this
*packed resources data* at run-time.

The :ref:`config_type_python_executable.packed_resources_load_mode` attribute
can be used in config files to control how this resources data should be
read.

Available Resource Data Load Modes
==================================

Embedded
--------

The *embedded* resources load mode (the default) will embed raw resources
data into the binary and it will be read from memory at run-time.

This mode is necessary to achieve self-contained, single-file executables.
This mode is also useful for single executable applications, where only
a single executable file embeds a Python interpreter.

This mode is also likely the fastest mode, as no explicit filesystem I/O
needs to be performed to reference resources data at run-time.

Binary Relative Memory Mapped File
----------------------------------

The *binary relative memory mapped file* load mode will write resources data
into a standalone file that is installed next to the built binary. At run-time,
that file will be memory mapped and memory mapped I/O will be used.

This mode is useful for multiple executable applications, as it enables
the resources data to be shared across executables without bloating total
distribution size.

Here's an example:

.. code-block:: python

   def make_exe():
       dist = default_python_distribution()

       exe = dist.to_python_executable(
           name = "myapp",
       )

       # Write and load resources from a "myapp.pypacked" file next to
       # the executable.
       exe.packed_resources_load_mode = "binary-relative-memory-mapped:myapp.pypacked"

       return exe

None / Disabled
---------------

The resources load mode of ``none`` will disable the writing and loading
of this *packed resources data*. This effectively means ``OxidizedFinder``
can't load anything by default.

This mode can be useful to produce a binary that behaves like ``python``,
without PyOxidizer's special run-time code. (See
:ref:`packaging_python_executable` for more on this topic.)

If this mode is in use, you will need to enable Python's filesystem
importer (:ref:`config_type_python_interpreter_config_filesystem_importer`)
or define custom Rust code to have ``OxidizedFinder`` *index* resources
or else the embedded Python interpreter will fail to initialize due to
missing modules.
