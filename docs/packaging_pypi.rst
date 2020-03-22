.. _packaging_pypi:

Packaging an Application from a PyPI Package
============================================

In this section, we'll show how to package the
`pyflakes <https://pypi.org/project/pyflakes/>`_ program using a published
PyPI package. (Pyflakes is a Python linter.)

First, let's create an empty project::

   $ pyoxidizer init-config-file pyflakes

Next, we need to edit the :ref:`configuration file <config_files>` to tell
PyOxidizer about pyflakes. Open the ``pyflakes/pyoxidizer.bzl`` file in your
favorite editor.

Find the ``make_exe()`` function. This function returns a
:ref:`PythonExecutable <config_python_executable>` instance which defines
a standalone executable containing Python. This function is a registered
*target*, which is a named entity that can be individually built or run.
By returning a ``PythonExecutable`` instance, this function/target is saying
*build an executable containing Python*.

The ``PythonExecutable`` type holds all state needed to package and run
a Python interpreter. This includes low-level interpreter configuration
settings to which Python resources (like source and bytecode modules)
are embedded in that executable binary. This type exposes an
:ref:`add_in_memory_python_resources() <config_python_executable_add_in_memory_python_resources>`
method which adds an iterable of objects representing Python resources to the
set of embedded resources.

Elsewhere in this function, the ``dist`` variable holds an instance of
:ref:`PythonDistribution <config_python_distribution>`. This type
represents a Python distribution, which is a fancy way of saying
*an implementation of Python*. In addition to defining the files
constituting that distribution, a ``PythonDistribution`` exposes
methods for performing Python packaging. One of those methods is
:ref:`pip_install() <config_python_distribution_pip_install>`,
which invokes ``pip install`` using that Python distribution.

To add a new Python package to our executable, we call
``dist.pip_install()`` then add the results to our ``PythonExecutable``
instance. This is done like so:

.. code-block:: python

   exe.add_in_memory_python_resources(dist.pip_install(["pyflakes==2.1.1"]))

The inner call to ``dist.pip_install()`` will effectively run
``pip install pyflakes==2.1.1`` and collect a set of installed
Python resources (like module sources and bytecode data) and return
that as an iterable data structure. The ``exe.add_in_memory_python_resources()``
call will then embed these resources in the built executable binary.

Next, we tell PyOxidizer to run ``pyflakes`` when the interpreter is executed:

.. code-block:: python

   run_eval="from pyflakes.api import main; main()",

This says to effectively run the Python code
``eval(from pyflakes.api import main; main())`` when the embedded interpreter
starts.

The new ``make_exe()`` function should look something like the following (with
comments removed for brevity):

.. code-block:: python

   def make_exe():
       dist = default_python_distribution()

       config = PythonInterpreterConfig(
           run_eval="from pyflakes.api import main; main()",
       )

       exe = dist.to_python_executable(
           name="pyflakes",
           config=config,
           extension_module_filter="all",
           include_sources=True,
           include_resources=False,
           include_test=False,
       )

       exe.add_in_memory_python_resources(dist.pip_install(["pyflakes==2.1.1"]))

       return exe

With the configuration changes made, we can build and run a ``pyflakes``
native executable::

   # From outside the ``pyflakes`` directory
   $ pyoxidizer run --path /path/to/pyflakes/project -- /path/to/python/file/to/analyze

   # From inside the ``pyflakes`` directory
   $ pyoxidizer run -- /path/to/python/file/to/analyze

   # Or if you prefer the Rust native tools
   $ cargo run -- /path/to/python/file/to/analyze

By default, ``pyflakes`` analyzes Python source code passed to it via
stdin.