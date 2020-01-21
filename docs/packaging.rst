.. _packaging:

====================
Packaging User Guide
====================

So you want to package a Python application using ``PyOxidizer``? You've come
to the right place to learn how! Read on for all the details on how to
*oxidize* your Python application!

First, you'll need to install ``PyOxidizer``. See :ref:`installing` for
instructions.

Creating a PyOxidizer Project
=============================

The process for *oxidizing* every Python application looks the same: you
start by creating a new ``PyOxidizer`` configuration file via the
``pyoxidizer init-config-file`` command::

   # Create a new configuration file in the directory "pyapp"
   $ pyoxidizer init-config-file pyapp

Behind the scenes, ``PyOxidizer`` works by leveraging a Rust project to
build binaries embedding Python. The auto-generated project simply
instantiates and runs an embedded Python interpreter. If you would like
your built binaries to offer more functionality, you can create a minimal
Rust project to embed a Python interpreter and customize from there::

   # Create a new Rust project for your application in ~/src/myapp.
   $ pyoxidizer init-rust-project ~/src/myapp

The auto-generated configuration file and Rust project will alunch a Python
REPL by default. And the ``pyoxidizer`` executable will look in the current
directory for a ``pyoxidizer.bzl`` configuration file. Let's test that the
new configuration file or project works::

   $ pyoxidizer run
   ...
      Compiling pyapp v0.1.0 (/home/gps/src/pyapp)
       Finished dev [unoptimized + debuginfo] target(s) in 53.14s
   writing executable to /home/gps/src/pyapp/build/x86_64-unknown-linux-gnu/debug/exe/pyapp
   >>>

If all goes according to plan, you just built a Rust executable which
contains an embedded copy of Python. That executable started an interactive
Python debugger on startup. Try typing in some Python code::

   >>> print("hello, world")
   hello, world

It works!

(To exit the REPL, press CTRL+d or CTRL+z or ``import sys; sys.exit(0)`` from
the REPL.)

.. note::

   If you have built a Rust project before, the output from building a
   ``PyOxidizer`` application may look familiar to you. That's because under the
   hood Cargo - Rust's package manager and build system - is doing a lot of the
   work to build the application. If you are familiar with Rust development,
   you can use ``cargo build`` and ``cargo run`` directly. However, Rust's
   build system is only responsible for build binaries and some of the
   higher-level functionality from ``PyOxidizer``'s configuration files (such
   as application packaging) will likely not be performed unless tweaks are
   made to the Rust project's ``build.rs``.

Now that we've got a new project, let's customize it to do something useful.

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
*an implementation of Python*. In addition to defining the files
constituting that distribution, a ``PythonDistribution`` exposes
methods for performing Python packaging. One of those methods is
:ref:`pip_install() <config_python_distribution_pip_install>`,
which invokes ``pip install`` using that Python distribution.

To add a new Python package to our executable, we call
``dist.pip_install()`` then add the results to our ``PythonExecutable``
instance. This is done like so:

.. code-block:: python

   exe.add_python_resources(dist.pip_install(["pyflakes==2.1.1"]))

The inner call to ``dist.pip_install()`` will effectively run
``pip install pyflakes==2.1.1`` and collect a set of installed
Python resources (like module sources and bytecode data) and return
that as an iterable data structure. The ``exe.add_python_resources()``
call will then embed these resources in the built executable binary.

Next, we tell PyOxidizer to run ``pyflakes`` when the interpreter is executed:

.. code-block:: python

   python_run_mode = python_run_mode_eval("from pyflakes.api import main; main()")

This says to effectively run the Python code
``eval(from pyflakes.api import main; main())`` when the embedded interpreter
starts.

The new ``make_exe()`` function should look something like the following (with
comments removed for brevity):

.. code-block:: python

   def make_exe():
       dist = default_python_distribution()

       python_config = PythonInterpreterConfig()

       python_run_mode = python_run_mode_eval("from pyflakes.api import main; main()")

       exe = PythonExecutable(
           name="pyflakes",
           distribution=dist,
           config=python_config,
           run_mode=python_run_mode,
           extension_module_filter="all",
           include_sources=True,
           include_resources=False,
           include_test=False,
       )

       exe.add_python_resources(dist.pip_intsall(["pyflakes==2.1.1"]))

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

What Can Go Wrong
=================

Ideally, packaging your Python application and its dependencies *just works*.
Unfortunately, we don't live in an ideal world.

PyOxidizer breaks various assumptions about how Python applications are
built and distributed. When attempting to package your application, you will
inevitably run into problems due to incompatibilities with PyOxidizer.

The :ref:`pitfalls` documentation can serve as a guide to identify and work
around these problems.

Packaging Additional Files
==========================

By default PyOxidizer will embed Python resources such as modules into
the compiled executable. This is the ideal method to produce distributable
Python applications because it can keep the entire application self-contained
to a single executable and can result in
:ref:`performance wins <better_performance>`.

But sometimes embedded resources into the binary isn't desired or doesn't
work. Fear not: PyOxidizer has you covered!

Let's give an example of this by attempting to package
`black <https://github.com/python/black>`_, a Python code formatter.

We start by creating a new project::

   $ pyoxidizer init-config-file black

Then edit the ``pyoxidizer.bzl`` file to have the following:

.. code-block:: python

   def make_exe():
       dist = default_python_distribution()
       python_config = PythonInterpreterConfig()
       python_run_mode = python_run_mode_module("black")

       exe = PythonExecutable(
           name="black",
           distribution=dist,
           resources=embedded,
           config=python_config,
           run_mode=python_run_mode,
       )

       exe.add_python_resources(dist.pip_intsall(["black==19.3b0"]))

       return exe

Then let's attempt to build the application::

   $ pyoxidizer build --path black
   processing config file /home/gps/src/black/pyoxidizer.bzl
   resolving Python distribution...
   ...

Looking good so far!

Now let's try to run it::

   $ pyoxidizer run --path black
   Traceback (most recent call last):
     File "black", line 46, in <module>
     File "blib2to3.pygram", line 15, in <module>
   NameError: name '__file__' is not defined
   SystemError

Uh oh - that's didn't work as expected.

As the error message shows, the ``blib2to3.pygram`` module is trying to
access ``__file__``, which is not defined. As explained by :ref:`no_file`,
``PyOxidizer`` doesn't set ``__file__`` for modules loaded from memory. This is
perfectly legal as Python doesn't mandate that ``__file__`` be defined. So
``black`` (and every other Python file assuming the existence of ``__file__``)
is arguably buggy.

Let's assume we can't easily change the offending source code to work around
the issue.

To fix this problem, we change the configuration file to install ``black``
relative to the built application. This requires changing our approach a
little. Before, we ran ``dist.pip_install()`` from ``make_exe()`` to collect
Python resources and added them to a ``PythonEmbeddedResources`` instance.
This meant those resources were embedded in the self-contained
``PythonExecutable`` instance returned from ``make_exe()``.

Our auto-generated ``pyoxidizer.bzl`` file also contains an ``install``
*target* defined by the ``make_install()`` function. This target produces
an ``FileManifest``, which represents a collection of relative files
and their content. When this type is *resolved*, those files are manifested
on the filesystem. To package ``black``'s Python resources next to our
executable instead of embedded within it, we need to move the ``pip_install()``
invocation from ``make_exe()`` to ``make_install()``.

Change your configuration file to look like the following:

.. code-block:: python

   def make_python_dist():
       return default_python_distribution()

   def make_exe(dist):
       embedded = dist.to_embedded_resources(
           extension_module_filter='all',
           include_sources=True,
           include_resources=False,
           include_test=False,
       )

       python_config = PythonInterpreterConfig(
           sys_paths=["$ORIGIN/lib"],
       )
       python_run_mode = python_run_mode_module("black")

       return PythonExecutable(
           name="black",
           distribution=dist,
           resources=embedded,
           config=python_config,
           run_mode=python_run_mode,
       )


   def make_install(dist, exe):
       files = FileManifest()

       files.add_python_resource(".", exe)

       files.add_python_resources("lib", dist.pip_install(["black==19.3b0"]))

       return files

   register_target("python_dist", make_python_dist)
   register_target("exe", make_exe, depends=["python_dist"])
   register_target("install", make_install, depends=["python_dist", "exe"], default=True)

   resolve_targets()

There are a few changes here.

We added a new ``make_dist()`` function and ``python_dist`` *target* to
represent obtaining the Python distribution. This isn't strictly required,
but it helps avoid redundant work during execution.

The ``PythonInterpreterConfig`` construction adds a ``sys_paths=["$ORIGIN/lib"]``
argument. This argument says *adjust ``sys.path`` at run-time to include the
``lib`` directory next to the executable file*. It allows the Python
interpreter to import Python files on the filesystem instead of just from
memory.

The ``make_install()`` function/target has also gained a call to
``files.add_python_resources()``. This method call takes the Python resources
collected from running ``pip install black==19.3b0`` and adds them to the
``FileManifest`` instance under the ``lib`` directory. When the ``FileManifest``
is resolved, those Python resources will be manifested as files on the
filesystem (e.g. as ``.py`` and ``.pyc`` files).

With the new configuration in place, let's re-build the application::

   $ pyoxidizer build --path black install
   ...
   packaging application into /home/gps/src/black/build/apps/black/x86_64-unknown-linux-gnu/debug
   purging /home/gps/src/black/build/apps/black/x86_64-unknown-linux-gnu/debug
   copying /home/gps/src/black/build/target/x86_64-unknown-linux-gnu/debug/black to /home/gps/src/black/build/apps/black/x86_64-unknown-linux-gnu/debug/black
   resolving packaging state...
   installing resources into 1 app-relative directories
   installing 46 app-relative Python source modules to /home/gps/src/black/build/apps/black/x86_64-unknown-linux-gnu/debug/lib
   ...
   black packaged into /home/gps/src/black/build/apps/black/x86_64-unknown-linux-gnu/debug

If you examine the output, you'll see that various Python modules files were
written to the output directory, just as our configuration file requested!

Let's try to run the application::

   $ pyoxidizer run --path black --target install
   No paths given. Nothing to do 😴

Success!

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

Adding Extension Modules At Run-Time
====================================

Normally, Python extension modules are compiled into the binary as part
of the embedded Python interpreter.

``PyOxidizer`` also supports providing additional extension modules at run-time.
This can be useful for larger Rust applications providing extension modules
that are implemented in Rust and aren't built through normal Python
build systems (like ``setup.py``).

If the ``PythonConfig`` Rust struct used to construct an embedded Python
interpreter contains a populated ``extra_extension_modules`` field, the
extension modules listed therein will be made available to the Python
interpreter.

Please note that Python stores extension modules in a global variable.
So instantiating multiple interpreters via the ``pyembed`` interfaces may
result in duplicate entries or unwanted extension modules being exposed to
the Python interpreter.

Masquerading As Other Packaging Tools
=====================================

Tools to package and distribute Python applications existed several
years before ``PyOxidizer``. Many Python packages have learned to perform
special behavior when the _fingerprint* of these tools is detected at
run-time.

First, ``PyOxidizer`` has its own fingerprint: ``sys.oxidized = True``. The
presence of this attribute can indicate an application running with
``PyOxidizer``. Other applications are discouraged from defining this
attribute.

Since ``PyOxidizer``'s run-time behavior is similar to other packaging
tools, ``PyOxidizer`` supports falsely identifying itself as these other
tools by emulating their fingerprints.

The ``EmbbedPythonConfig`` configuration section defines the
boolean flag ``sys_frozen`` to control whether ``sys.frozen = True``
is set. This can allow ``PyOxidizer`` to advertise itself as a *frozen*
application.

In addition, the ``sys_meipass`` boolean flag controls whether a
``sys._MEIPASS = <exe directory>`` attribute is set. This allows
``PyOxidizer`` to masquerade as having been built with PyInstaller.

.. warning::

   Masquerading as other packaging tools is effectively lying and can
   be dangerous, as code relying on these attributes won't know if
   it is interacting with ``PyOxidizer`` or some other tool. It is
   recommended    to only set these attributes to unblock enabling
   packages to work with ``PyOxidizer`` until other packages learn to
   check for ``sys.oxidized = True``. Setting ``sys._MEIPASS`` is
   definitely the more risky option, as a case can be made that
   PyOxidizer should set ``sys.frozen = True`` by default.
