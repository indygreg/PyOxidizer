.. _getting_started:

===============
Getting Started
===============

.. _installing:

Installing
==========

Installing Rust
---------------

PyOxidizer is a Rust application and requires Rust (1.36 or newer) to be
installed in order to build PyOxidizer itself as well as Python application
binaries.

You can verify your installed version of Rust by running::

   $ rustc --version
   rustc 1.38.0 (625451e37 2019-09-23)

If you don't have Rust installed, https://www.rust-lang.org/ has very detailed
instructions on how to install it.

Rust releases a new version every 6 weeks and language development moves
faster than other programming languages. It is common for the Rust packages
provided by common package managers to lag behind the latest Rust release by
several releases. For that reason, use of the ``rustup`` tool for managing
Rust is highly recommended.

If you are a security paranoid individual and don't want to follow the
official ``rustup`` install instructions involving a ``curl | sh`` (your
paranoia is understood), you can find instructions for alternative installation
methods at https://github.com/rust-lang/rustup.rs/#other-installation-methods.

Other System Dependencies
-------------------------

You will need a working C compiler/toolchain in order to build some Rust
crates and their dependencies. If Rust cannot find a C compiler, it should
print a message at build time and give you instructions on how to install one.

There is a known issue with PyOxidizer on Fedora 30+ that will require you
to install the ``libxcrypt-compat`` package to avoid an error due to a missing
``libcrypt.so.1`` file. See https://github.com/indygreg/PyOxidizer/issues/89
for more info.

Installing PyOxidizer
---------------------

PyOxidizer can be installed from its latest published crate::

   $ cargo install pyoxidizer

From a Git repository using cargo::

   # The latest commit in source control.
   $ cargo install --git https://github.com/indygreg/PyOxidizer.git --branch main pyoxidizer

   $ A specific release
   $ cargo install --git https://github.com/indygreg/PyOxidizer.git --tag <TAG> pyoxidizer

Or by cloning the Git repository and building the project locally::

   $ git clone https://github.com/indygreg/PyOxidizer.git
   $ cd PyOxidizer
   $ cargo install --path pyoxidizer

.. note::

   PyOxidizer's project policy is for the ``main`` branch to be stable. So it
   should always be relatively safe to use ``main`` instead of a released
   version.

.. danger::

   A ``cargo build`` from the repository root directory will likely fail due
   to how some of the Rust crates are configured.

   See :ref:`rust_cargo_source_checkouts` for instructions on how to invoke
   ``cargo``.

Once the ``pyoxidizer`` executable is installed, try to run it::

   $ pyoxidizer
   PyOxidizer 0.5
   Gregory Szorc <gregory.szorc@gmail.com>
   Build and distribute Python applications

   USAGE:
       pyoxidizer [SUBCOMMAND]

   ...

Congratulations, PyOxidizer is installed! Now let's move on to using it.

High-Level Project Lifecycle
============================

``PyOxidizer`` exposes various functionality through the interaction
of ``pyoxidizer`` commands and configuration files.

The first step of any project is to create it. This is achieved
with a ``pyoxidizer init-*`` command to create files required by
``PyOxidizer``.

After that, various ``pyoxidizer`` commands can be used to evaluate
configuration files and perform actions from the evaluated file.
``PyOxidizer`` provides functionality for building binaries, installing
files into a directory tree, and running the results of build actions.

Your First PyOxidizer Project
=============================

The ``pyoxidizer init-config-file`` command will create a new PyOxidizer
configuration file in a directory of your choosing::

   $ pyoxidizer init-config-file pyapp

This should have printed out details on what happened and what to do next.
If you actually ran this in a terminal, hopefully you don't need to continue
following the directions here as the printed instructions are sufficient!
But if you aren't, keep reading.

The default configuration created by ``pyoxidizer init-config-file`` will
produce an executable that embeds Python and starts a Python REPL by default.
Let's test that::

   $ cd pyapp
   $ pyoxidizer run
   resolving 1 targets
   resolving target exe
   ...
       Compiling pyapp v0.1.0 (/tmp/pyoxidizer.nv7QvpNPRgL5/pyapp)
        Finished dev [unoptimized + debuginfo] target(s) in 26.07s
   writing executable to /home/gps/src/pyapp/build/x86_64-unknown-linux-gnu/debug/exe/pyapp
   >>>

If all goes according to plan, you just started a Rust executable which
started a Python interpreter, which started an interactive Python debugger!
Try typing in some Python code::

   >>> print("hello, world")
   hello, world

It works!

(To exit the REPL, press CTRL+d or CTRL+z.)

Continue reading :ref:`managing_projects` to learn more about the
``pyoxidizer`` tool. Or read on for a preview of how to customize your
application's behavior.

The ``pyoxidizer.bzl`` Configuration File
=========================================

The most important file for a ``PyOxidizer`` project is the ``pyoxidizer.bzl``
configuration file. This is a Starlark file evaluated in a context that
provides special functionality for ``PyOxidizer``.

Starlark is a Python-like interpreted language and its syntax and semantics
should be familiar to any Python programmer.

From a high-level, ``PyOxidizer``'s configuration files define named
``targets``, which are callable functions associated with a name - the
*target* - that resolve to an entity. For example, a configuration file
may define a ``build_exe()`` function which returns an object representing
a standalone executable file embedding Python. The ``pyoxidizer build``
command can be used to evaluate just that target/function.

Target functions can call out to other target functions. For example, there
may be an ``install`` target that creates a set of files composing a full
application. Its function may evaluate the ``exe`` target to produce an
executable file.

See :ref:`config_files` for comprehensive documentation of ``pyoxidizer.bzl``
files and their semantics.

Customizing Python and Packaging Behavior
=========================================

Embedding Python in a Rust executable and starting a REPL is cool and all.
But you probably want to do something more exciting.

The autogenerated ``pyoxidizer.bzl`` file created as part of running
``pyoxidizer init-config-file`` defines how your application is configured
and built. It controls everything from what Python distribution to use,
which Python packages to install, how the embedded Python interpreter is
configured, and what code to run in that interpreter.

Open ``pyoxidizer.bzl`` in your favorite editor and find the line passing a
``run_repl`` argument, which configures the embedded interpreter to run
a Python REPL. Let's replace that line with the following:

.. code-block:: python

   run_eval="import uuid; print(uuid.uuid4())",

We're now telling the interpreter to run the Python statement
``eval(import uuid; print(uuid.uuid4())`` when it starts. Test that out::

   $ pyoxidizer run
   ...
      Compiling pyapp v0.1.0 (/home/gps/src/pyapp)
       Finished dev [unoptimized + debuginfo] target(s) in 3.92s
        Running `target/debug/pyapp`
   writing executable to /home/gps/src/pyapp/build/x86_64-unknown-linux-gnu/debug/exe/pyapp
   96f776c8-c32d-48d8-8c1c-aef8a735f535

It works!

This is still pretty trivial. But it demonstrates how the ``pyoxidizer.bzl``
is used to influence the behavior of built executables.

Let's do something a little bit more complicated, like package an existing
Python application!

Find the ``exe = dist.to_python_executable(`` line in the
``pyoxidizer.bzl`` file. Let's add a new line to ``make_exe()`` just
below where ``exe`` is assigned:

.. code-block:: python

   exe.add_in_memory_python_resources(exe.pip_install(["pyflakes==2.1.1"]))

In addition, replace the ``run_*`` argument to execute ``pyflakes``:

.. code-block:: python

   run_eval="from pyflakes.api import main; main()",

Now let's try building and running the new configuration::

   $ pyoxidizer run -- --help
   ...
      Compiling pyapp v0.1.0 (/home/gps/src/pyapp)
       Finished dev [unoptimized + debuginfo] target(s) in 5.49s
   writing executable to /home/gps/src/pyapp/build/x86_64-unknown-linux-gnu/debug/exe/pyapp
   Usage: pyapp [options]

   Options:
     --version   show program's version number and exit
     -h, --help  show this help message and exit

You've just produced an executable for ``pyflakes``!

There are far more powerful packaging and configuration settings available.
Read all about them at :ref:`config_files` and :ref:`packaging`. Or continue
on to :ref:`managing_projects` to learn more about the ``pyoxidizer`` tool.
