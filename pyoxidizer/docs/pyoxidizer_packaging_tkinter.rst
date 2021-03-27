.. _packaging_tkinter:

===================================
Using the ``tkinter`` Python Module
===================================

The `tkinter <https://docs.python.org/3/library/tkinter.html>`_ Python
standard library module/package provides a Python interface to
tcl/tk/tkinter. This interface allows you to create GUI applications.

PyOxidizer has partial support for using ``tkinter``. Since ``tkinter``
isn't a commonly used Python feature, you must opt in to enabling it.

.. _packaging_installing_tcl_files:

Installing tcl Files
====================

``tkinter`` requires both a Python extension module compiled against
tcl/tk and tcl support files to be loaded at run-time.

All the
:ref:`built-in Python distributions <packaging_available_python_distributions>`
shipping with PyOxidizer provide ``tkinter`` support with the exception of the
Windows ``standalone_static`` distributions.

However, the tcl support files aren't installed by default.

To install tcl support files, you will need to set the
:ref:`config_type_python_executable_tcl_files_path` attribute of a
:ref:`config_type_python_executable` instance to the directory you
want to install these files into. e.g.

.. code-block:: python

   def make_exe(dist):
       exe = dist.to_python_executable(name="myapp")
       exe.tcl_files_path = "lib"

       return exe

When ``tcl_files_path`` is set to a non-``None`` value, the tcl files
required by ``tkinter`` are installed in that directory and the built
executable will automatically set the ``TCL_LIBRARY`` environment variable
at run-time so the tcl interpreter uses those files.

.. _packaging_tcl_files_self_contained:

tcl Files Prevent Self-Contained Executables
============================================

The tcl interpreter needs to load various files off the filesystem
at run-time. PyOxidizer does not (yet) support embedding these files in
the binary and loading them from memory or extracting them at run-time.

So if you need to use ``tkinter``, you cannot have a single-file executable
that works without a dependency on tcl files elsewhere on the filesystem.
