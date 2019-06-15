.. _packaging:

====================
Packaging User Guide
====================

So you want to package a Python application using PyOxidizer? You've come
to the right place to learn how! Read on for all the details on how to
*oxidize* your Python application!

First, you'll need to install PyOxidizer. See :ref:`installing` for
instructions.

Creating a PyOxidizer Project
=============================

Behind the scenes, PyOxidizer works by creating a Rust project which embeds
and runs a Python interpreter.

The process for *oxidizing* every Python application looks the same: you
start by creating a new [Rust] project with the PyOxidizer scaffolding.
The ``pyoxidizer init`` command does this::

   # Create a new project named "pyapp" in the directory "pyapp"
   $ pyoxidizer init pyapp

   # Create a new project named "myapp" in the directory "~/src/myapp"
   $ pyoxidizer init ~/src/myapp

The default project created by ``pyoxidizer init`` will produce an executable
that embeds Python and starts a Python REPL. Let's test that::

   $ pyoxidizer run pyapp
   no existing PyOxidizer artifacts found
   processing config file /home/gps/src/pyapp/pyoxidizer.toml
   resolving Python distribution...
      Compiling pyapp v0.1.0 (/home/gps/src/pyapp)
       Finished dev [unoptimized + debuginfo] target(s) in 53.14s
        Running `target/debug/pyapp`
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
   PyOxidizer application may look familiar to you. That's because under the
   hood Cargo - Rust's package manager and build system - is doing most of the
   work to build the application. The ``build`` and ``run`` ``pyoxidizer``
   commands are essentially minimal wrappers around ``cargo`` commands. If you
   are familiar with Rust development, feel free to use ``cargo build`` and
   ``cargo run`` directly.

If you are curious about what's inside newly-created projects, read
:ref:`new_project_layout`.

Now that we've got a new project, let's customize it to do something useful.

Packaging an Application from a PyPI Package
============================================

In this section, we'll show how to package the
`pyflakes <https://pypi.org/project/pyflakes/>`_ program using a published
PyPI package. (Pyflakes is a Python linter.)

First, let's create an empty project::

   $ pyoxidizer init pyflakes

Next, we need to edit the :ref:`configuration file <config_files>` to tell
PyOxidizer about pyflakes. Open the ``pyflakes/pyoxidizer.toml`` file in your
favorite editor.

We first tell PyOxidizer to add the ``pyflakes`` Python package by adding the
following lines:

.. code-block:: toml

   [[python_packages]]
   type = "pip-install-simple"
   package = "pyflakes==2.1.1"

This creates a packaging rule that essentially translates to running
``pip install pyflakes==2.1.1`` and then finds and packages the files installed
by that command.

Next, we tell PyOxidizer to run pyflakes when the interpreter is executed.
Find the ``[[python_run]]`` section and change its contents to the following:

.. code-block:: toml

   [[python_run]]
   mode = "eval"
   code = "from pyflakes.api import main; main()"

This says to effectively run the Python code
``eval(from pyflakes.api import main; main())`` when the embedded interpreter
starts.

The new ``pyoxidizer.toml`` file should look something like:

.. code-block:: toml

   # Multiple [[python_distribution]] sections elided for brevity.

   [[embedded_python_config]]
   program_name = "pyflakes"
   raw_allocator = "jemalloc"

   [[python_packages]]
   type = "stdlib-extensions-policy"
   policy = "all"

   [[python_packages]]
   type = "stdlib"
   include_source = false

   [[python_packages]]
   type = "pip-install-simple"
   package = "pyflakes==2.1.1"

   [[python_run]]
   mode = "eval"
   code = "from pyflakes.api import main; main()"

With the configuration changes made, we can build and run a ``pyflakes``
native executable::

   # From outside the ``pyflakes`` directory
   $ pyoxidizer run /path/to/pyflakes/project -- /path/to/python/file/to/analyze

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
