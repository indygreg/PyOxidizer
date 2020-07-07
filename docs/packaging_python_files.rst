.. _packaging_python_files:

======================
Packaging Python Files
======================

The most important packaged :ref:`resource type <packaging_resource_types>`
is arguably Python files: source modules, bytecode modules, and
extension modules.

For PyOxidizer to recognize these Python resources as Python resources
(as opposed to regular files), you will need to use the methods on the
:ref:`PythonDistribution <config_python_distribution>` Starlark type
to use the Python distribution to scan for resources, possibly performing
a Python packaging action (such as invoking ``pip install``) along the way.

This documentation covers the available methods and how they can be
used.

.. _packaging_python_executable_python_resource_methods:

``PythonExecutable`` Python Resources Methods
=============================================

The ``PythonExecutable`` Starlark type has the following methods that
can be called to perform an action and obtain an iterable of objects
representing discovered resources:

:ref:`pip_install(...) <config_python_executable_pip_install>`
   Invokes ``pip install`` with specified arguments and collects all
   resources installed by that process.

:ref:`read_package_root(...) <config_python_executable_read_package_root>`
   Recursively scans a filesystem directory for Python resources in a
   typical Python installation layout.

:ref:`setup_py_install(...) <config_python_executable_setup_py_install>`
   Invokes ``python setup.py install`` for a given path and collects
   resources installed by that process.

:ref:`read_virtualenv(...) <config_python_executable_read_virtualenv>`
   Reads Python resources present in an already populated virtualenv.

Typically, the Starlark types resolved by these method calls are
passed into a method that adds the resource to a to-be-generated
entity, such as the :ref:`PythonExecutable <config_python_executable>`
Starlark type. See :ref:`packaging_routing_resources` for more.

The following sections demonstrate common use cases.

.. _packaging_from_pypi_package:

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
:ref:`add_python_resources() <config_python_executable_add_python_resources>`
method which adds an iterable of objects representing Python resources to the
set of embedded resources.

Elsewhere in this function, the ``dist`` variable holds an instance of
:ref:`PythonDistribution <config_python_distribution>`. This type
represents a Python distribution, which is a fancy way of saying
*an implementation of Python*.

One of the methods exposed by ``PythonExecutable`` is
:ref:`pip_install() <config_python_executable_pip_install>`, which
invokes ``pip install`` with settings to target the built executable.

To add a new Python package to our executable, we call
``exe.pip_install()`` then add the results to our ``PythonExecutable``
instance. This is done like so:

.. code-block:: python

   exe.add_python_resources(exe.pip_install(["pyflakes==2.1.1"]))

The inner call to ``exe.pip_install()`` will effectively run
``pip install pyflakes==2.1.1`` and collect a set of installed
Python resources (like module sources and bytecode data) and return
that as an iterable data structure. The ``exe.add_python_resources()``
call will then teach the built executable binary about the existence of
these resources. Many resource types will be embedded in the binary
and loaded from binary. But some resource types (notably compiled
extension modules) may be installed next to the built binary and
loaded from the filesystem.

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

       exe.add_python_resources(exe.pip_install(["pyflakes==2.1.1"]))

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

.. _packaging_from_virtualenv:

Packaging an Application from an Existing Virtualenv
====================================================

This scenario is very similar to the above example. So we'll only briefly
describe what to do so we don't repeat ourselves.::

   $ pyoxidizer init-config-file /path/to/myapp

Now edit the ``pyoxidizer.bzl`` so the ``make_exe()`` function look like the
following:

.. code-block:: python

   def make_exe():
       dist = default_python_distribution()

       config = PythonInterpreterConfig(
           run_eval="from myapp import main; main()",
       )

       exe = dist.to_python_executable(
           name="myapp",
           config=config,
           extension_module_filter="all",
           include_sources=True,
           include_resources=False,
           include_test=False,
       )

       exe.add_python_resources(exe.read_virtualenv("/path/to/virtualenv"))

       return exe

Of course, you need a populated virtualenv!::

   $ python3.8 -m venv /path/to/virtualenv
   $ /path/to/virtualenv/bin/pip install -r /path/to/requirements.txt

Once all the pieces are in place, simply run ``pyoxidizer`` to build and
run the application::

    $ pyoxidizer run --path /path/to/myapp

.. warning::

   When consuming a pre-populated virtualenv, there may be compatibility
   differences between the Python distribution used to populate the virtualenv
   and the Python distributed used by PyOxidizer at build and application run
   time.

   For best results, it is recommended to use a packaging method like
   ``pip_install(...)`` or ``setup_py_install(...)`` to use PyOxidizer's
   Python distribution to invoke Python's packaging tools.

.. _packaging_from_local_python_package:

Packaging an Application from a Local Python Package
====================================================

Say you have a Python package/application in a local directory. It follows
the typical Python package layout and has a ``setup.py`` file and Python
files in sub-directories corresponding to the package name. e.g.::

   setup.py
   mypackage/__init__.py
   mypackage/foo.py

You have a number of choices as to how to proceed here. Again, the
workflow is very similar to what was explained above. The main difference
is the content of the ``pyoxidizer.bzl`` file and the exact
:ref:`method <packaging_python_executable_python_resource_methods>` to call
to obtain the Python resources.

You could use ``pip install <local path>`` to use ``pip`` to process a local
filesystem path:

.. code-block:: python

   exe.add_python_resources(exe.pip_install(["/path/to/local/package"]))

If the ``pyoxidizer.bzl`` file is in the same directory as the directory you
want to process, you can derive the absolute path to this directory via the
:ref:`CWD <config_cwd>` Starlark variable:

.. code-block:: python

   exe.add_python_resources(exe.pip_install([CWD]))

If you don't want to use ``pip`` and want to run ``setup.py`` directly,
you can do so:

.. code-block:: python

   exe.add_python_resources(exe.setup_py_install(package_path=CWD))

Or if you don't want to run a Python packaging tool at all and just
scan a directory tree for Python files:

.. code-block:: python

   exe.add_python_resources(exe.read_package_root(CWD, ["mypackage"]))

.. note::

   In this mode, all Python resources must already be in place in their
   final installation layout for things to work correctly. Many ``setup.py``
   files perform additional actions such as compiling Python extension
   modules, installing additional files, dynamically generating some files,
   or changing the final installation layout.

   For best results, use a packaging method that invokes a Python packaging
   tool (like ``pip_install(...)`` or ``setup_py_install(...)``.
