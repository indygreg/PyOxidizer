.. _packaging_creating_projects:

=============================
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

The auto-generated configuration file and Rust project will launch a Python
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
