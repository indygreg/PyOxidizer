.. _packaging_additional_files:

====================================================
Packaging Python Files Next to the Built Application
====================================================

By default PyOxidizer will embed Python resources such as modules into
the compiled executable. This is the ideal method to produce distributable
Python applications because it can keep the entire application self-contained
to a single executable and can result in
:ref:`performance wins <packaging_performance>`.

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

       config = PythonInterpreterConfig(
           run_module="black",
       )

       exe = dist.to_python_executable(
           name="black",
       )

       exe.add_in_memory_python_resources(exe.pip_install(["black==19.3b0"]))

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
little. Before, we ran ``exe.pip_install()`` from ``make_exe()`` to collect
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
       python_config = PythonInterpreterConfig(
           run_module="black",
           sys_paths=["$ORIGIN/lib"],
       )

       return dist.to_python_executable(
           name="black",
           config=python_config,
           extension_module_filter='all',
           include_sources=True,
           include_resources=False,
           include_test=False,
       )


   def make_install(exe):
       files = FileManifest()

       files.add_python_resource(".", exe)

       files.add_python_resources("lib", exe.pip_install(["black==19.3b0"]))

       return files

   register_target("python_dist", make_python_dist)
   register_target("exe", make_exe, depends=["python_dist"])
   register_target("install", make_install, depends=["exe"], default=True)

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
