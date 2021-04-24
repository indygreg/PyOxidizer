.. py:currentmodule:: starlark_pyoxidizer

.. _packaging_additional_files:

==============================================
Packaging Files Instead of In-Memory Resources
==============================================

By default, PyOxidizer will *classify* files into typed resources and
attempt to load these resources from memory (with the exception of
compiled extension modules, which require special treatment). Please
read :ref:`packaging_resources`, specifically
:ref:`packaging_resources_classified_files` and
:ref:`packaging_resource_locations` for more on the concepts of
*classification* and *resource locations*.

This is the ideal packaging method because it keeps the entire application
self-contained and can result in
:ref:`performance wins <packaging_performance>` at run-time.

However, sometimes this approach isn't desired or flat out doesn't work.
Fear not: PyOxidizer has you covered.

Examples of Packaging Failures
==============================

Let's give some concrete examples of how PyOxidizer's default packaging
settings can fail.

.. _packaging_failure_black:

black
-----

Let's demonstrate a failure attempting to package
`black <https://github.com/python/black>`_, a Python code formatter.

We start by creating a new project::

   $ pyoxidizer init-config-file black

Then edit the ``pyoxidizer.bzl`` file to have the following:

.. code-block:: python

   def make_exe(dist):
       config = dist.make_python_interpreter_config()
       config.run_module = "black"

       exe = dist.to_python_executable(
           name = "black",
       )

       for resource in exe.pip_install(["black==19.3b0"]):
           resource.add_location = "in-memory"
           exe.add_python_resource(resource)

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
perfectly legal as Python doesn't mandate that ``__file__`` be defined. But
``black`` (and many other Python modules) assume ``__file__`` always exists.
So it is a problem we have to deal with.

.. _packaging_failure_numpy:

NumPy
-----

Let's attempt to package `NumPy <https://numpy.org/>`_, a popular Python
package used by the scientific computing crowd.

   $ pyoxidizer init-config-file numpy

Then edit the ``pyoxidizer.bzl`` file to have the following:

.. code-block:: python

   def make_exe(dist):
       policy = dist.make_python_packaging_policy()
       policy.resources_location_fallback = "filesystem-relative:lib"

       exe = dist.to_python_executable(
           name = "numpy",
           packaging_policy = policy,
       )

       for resource in exe.pip_download(["numpy==1.19.0"]):
           resource.add_location = "filesystem-relative:lib"
           exe.add_python_resource(resource)

       return exe

We did things a little differently from the ``black`` example above:
we're explicitly adding NumPy's resources into the ``filesystem-relative``
location so they are materialized as files instead of loaded from memory.
This is to demonstrate a separate failure mode.

Then let's attempt to build the application::

   $ pyoxidizer build --path numpy
   processing config file /home/gps/src/numpy/pyoxidizer.bzl
   resolving Python distribution...
   ...

Looking good so far!

Now let's try to run it::

   $ pyoxidizer run --path numpy
   ...
   Python 3.8.6 (default, Oct  3 2020, 20:48:20)
   [Clang 10.0.1 ] on linux
   Type "help", "copyright", "credits" or "license" for more information.
   >>> import numpy
   Traceback (most recent call last):
     File "numpy.core", line 22, in <module>
     File "numpy.core.multiarray", line 12, in <module>
     File "numpy.core.overrides", line 7, in <module>
   ImportError: libopenblasp-r0-ae94cfde.3.9.dev.so: cannot open shared object file: No such file or directory

   During handling of the above exception, another exception occurred:
   ...

That's not good! What happened?

Well, the hint is in the stack trace: ``libopenblasp-r0-ae94cfde.3.9.dev.so:
cannot open shared object file: No such file or directory``. So there's a file
named ``libopenblasp-r0-ae94cfde.3.9.dev.so`` that can't be found. Let's
look in our install layout::

   $ find numpy/build/x86_64-unknown-linux-gnu/debug/install/ | grep libopenblasp
   numpy/build/x86_64-unknown-linux-gnu/debug/install/lib/numpy/libs/libopenblasp-r0-ae94cfde
   numpy/build/x86_64-unknown-linux-gnu/debug/install/lib/numpy/libs/libopenblasp-r0-ae94cfde/3
   numpy/build/x86_64-unknown-linux-gnu/debug/install/lib/numpy/libs/libopenblasp-r0-ae94cfde/3/9
   numpy/build/x86_64-unknown-linux-gnu/debug/install/lib/numpy/libs/libopenblasp-r0-ae94cfde/3/9/dev.so

Well, we found some files, including a ``.so`` file! But the filename has been
mangled.

This filename mangling is actually a bug in PyOxidizer's file/resource
classification. See :ref:`pitfall_incorrect_resource_identification` and
:ref:`packaging_resources_classified_files` for more.

.. _packaging_installing_resources_on_the_filesystem:

Installing Classified Resources on the Filesystem
=================================================

In the :ref:`black <packaging_failure_black>` example above, we saw how
``black`` failed to run with modules imported from memory because of
``__file__`` not being defined.

In scenarios where in-memory resource loading doesn't work, the ideal
mitigation is to fix the offending Python modules so they can load
from memory. But this isn't always trivial or possible with 3rd party
dependencies.

Your next mitigation should be to attempt to place the resource on the
filesystem, next to the built binary.

This will require configuration file changes.

The goal of our new configuration is to materialize Python resources
associated with ``black`` on the filesystem instead of in memory.

Change your configuration file so ``make_exe()`` looks like the following:

.. code-block:: python

   def make_exe(dist):
       policy = dist.make_python_packaging_policy()
       policy.resources_location_fallback = "filesystem-relative:lib"

       python_config = dist.make_python_interpreter_config()
       python_config.run_module = "black"

       exe = dist.to_python_executable(
           name = "black",
           packaging_policy = policy,
           config = python_config,
       )

       for resource in exe.pip_install(["black==19.3b0"]):
           resource.add_location = "filesystem-relative:lib"
           exe.add_python_resource(resource)

       return exe

There are a few changes here.

We constructed a new :py:class:`PythonPackagingPolicy` via
:py:meth:`PythonDistribution.make_python_packaging_policy` and set
its :py:attr:`PythonPackagingPolicy.resources_location_fallback`
attribute to ``filesystem-relative-lib``. This allows us to install resources
on the filesystem, relative to the produced binary.

Next, in the ``for resource in exe.pip_install(...)`` loop, we set
``resource.add_location = "filesystem-relative:lib"``. What this does
is tell the subsequent call to
:py:meth:`PythonExecutable.add_python_resource` to add the resource
as a filesystem-relative resource in the ``lib`` directory.

With the new configuration in place, let's re-build and run the application::

   $ pyoxidizer run --path black
   ...
   adding extra file lib/toml-0.10.1.dist-info/top_level.txt to .
   installing files to /home/gps/tmp/myapp/build/x86_64-unknown-linux-gnu/debug/install
   No paths given. Nothing to do 😴

That ``No paths given`` output is from ``black``: it looks like the new
configuration worked!

If you examine the build output, you'll see a bunch of messages indicating
that extra files are being installed to the ``lib/`` directory. And if you
poke around in the ``install`` directory, you will in fact see all these
files.

In this configuration file, the Python distribution's files are all loaded
from memory but ``black`` resources (collected via ``pip install black``) are
materialized on the filesystem. All of the resources are indexed by PyOxidizer
at build time and that index is embedded into the built binary so
:ref:`oxidized_importer` can find and load resources more efficiently.

Because only some of the Python modules used by ``black`` have a dependency
on ``__file__``, it is probably possible to cherry pick exactly which
resources are materialized on the filesystem and minimize the number of
files present. We'll leave that as an exercise for the reader.

.. _packaging_installing_unclassified_files_on_the_filesystem:

Installing Unclassified Files on the Filesystem
===============================================

In :ref:`packaging_installing_resources_on_the_filesystem` we demonstrated
how to move *classified* resources from memory to the filesystem in order to
work around issues importing a module from memory.

Astute readers may have already realized that this workaround
(setting ``.add_location`` to ``filesystem-relative:...``) was attempted
in the :ref:`packaging_failure_numpy` failure example above. So this
workaround doesn't always work.

In cases where PyOxidizer's resource classifier or logic to materialize
those classified resources as files is failing (presumably due to bugs
in PyOxidizer), you can fall back to using *unclassified*, file-based
resources. See :ref:`packaging_resources_classified_files` for more
on *classified* versus *files* based resources.

Our approach here is to switch from *classified* to *files* packaging
mode. Using our NumPy example from above, change the ``make_exe()`` in
your configuration file to as follows:

.. code-block:: python

   def make_exe(dist):
       policy = dist.make_python_packaging_policy()
       policy.set_resource_handling_mode("files")
       policy.resources_location_fallback = "filesystem-relative:lib"

       python_config = dist.make_python_interpreter_config()
       python_config.module_search_paths = ["$ORIGIN/lib"]

       exe = dist.to_python_executable(
           name = "numpy",
           packaging_policy = policy,
           config = python_config,
       )

       for resource in exe.pip_download(["numpy==1.19.0"]):
           resource.add_location = "filesystem-relative:lib"
           exe.add_python_resource(resource)

       return exe

There are a few key lines here.

``policy.set_resource_handling_mode("files")`` calls a method on the
:py:class:`PythonPackagingPolicy` to set the resource handling
mode to *files*. This effectively enables :py:class:`File` based
resources to work. Without it, resource scanners won't emit
:py:class:`File` and attempts at adding :py:class:`File`
to a resource collection will fail.

Next, we enable file-based resource installs by setting
:py:attr:`PythonPackagingPolicy.resources_location_fallback`.

Another new line is ``python_config.module_search_paths = ["$ORIGIN/lib"]``.
This all-important line to set
:py:attr:`PythonInterpreterConfig.module_search_paths` effectively
installs the ``lib`` directory next to the executable on ``sys.path`` at
run-time. And as a side-effect of defining this attribute, Python's built-in
module importer is enabled (to supplement ``oxidized_importer``). This is
important because because when you are operating in *files* mode, resources
are indexed as *files* and not classified/typed resources. This means
``oxidized_importer`` doesn't recognize them as loadable Python modules.
But since you enable Python's standard importer and register ``lib/`` as
a search path, Python's standard importer will be able to find the ``numpy``
package at run-time.

Anyway, let's see if this actually works::

   $ pyoxidizer run --path numpy
   ...
   adding extra file lib/numpy.libs/libgfortran-2e0d59d6.so.5.0.0 to .
   adding extra file lib/numpy.libs/libopenblasp-r0-ae94cfde.3.9.dev.so to .
   adding extra file lib/numpy.libs/libquadmath-2d0c479f.so.0.0.0 to .
   adding extra file lib/numpy.libs/libz-eb09ad1d.so.1.2.3 to .
   installing files to /home/gps/tmp/myapp/build/x86_64-unknown-linux-gnu/debug/install
   Python 3.8.6 (default, Oct  3 2020, 20:48:20)
   [Clang 10.0.1 ] on linux
   Type "help", "copyright", "credits" or "license" for more information.
   >>> import numpy
   >>> numpy.__loader__
   <_frozen_importlib_external.SourceFileLoader object at 0x7f063da1c7f0>


It works!

Critically, we see that the formerly missing ``libopenblasp-r0-ae94cfde.3.9.dev.so``
file is being installed to the correct location. And we can confirm from the
``numpy.__loader__`` value that the standard library's module loader is
being used. Contrast with a standard library module::

   >>> import pathlib
   >>> pathlib.__loader__
   <OxidizedFinder object at 0x7f063dc8f8f0>

Enabling *files* mode and falling back to Python's importer is often a good
way of working around bugs in PyOxidizer's *resource handling*. But it isn't
bulletproof.

.. important::

   Please `file a bug report <https://github.com/indygreg/PyOxidizer/issues>`
   if you encounter any issues with PyOxidizer's handling of resources and
   paths.
