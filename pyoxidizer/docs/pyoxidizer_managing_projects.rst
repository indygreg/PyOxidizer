.. py:currentmodule:: starlark_pyoxidizer

.. _managing_projects:

====================================
The ``pyoxidizer`` Command Line Tool
====================================

The ``pyoxidizer`` command line tool is a frontend to the various
functionality of ``PyOxidizer``. See :ref:`components` for more
on the various components of ``PyOxidizer``.

.. _pyoxidizer_settings:

Settings
========

.. _pyoxidizer_cache:

Cache Directory
---------------

``pyoxidizer`` may need to download resources such as Python distributions
and Rust toolchains from the Internet. These resources are cached in a
per-user directory.

PyOxidizer chooses the first available directory from the following list
to use as the cache:

* The value of the environment variable ``PYOXIDIZER_CACHE_DIR``.
* ``$XDG_CACHE_HOME/pyoxidizer`` on Linux if ``XDG_CACHE_HOME`` is set.
* ``$HOME/.cache/pyoxidizer`` on Linux if ``HOME`` is set.
* ``$HOME/Library/Caches/pyoxidizer`` on macOS if ``HOME`` is set.
* ``{FOLDERID_LocalAppData}/pyoxidizer`` on Windows.
* ``~/.pyoxidizer/cache``

The ``pyoxidizer cache-clear`` command can be used to delete the contents
of the cache.

.. _pyoxidizer_managed_rust:

Managed Rust Toolchain
----------------------

PyOxidizer leverages the Rust programming language and its tooling
for building binaries embedding Python.

By default, PyOxidizer will automatically download and use Rust toolchains
(the Rust compiler, standard library, and Cargo) when their functionality is
needed. PyOxidizer will store these Rust toolchains in the configured
:ref:`cache <pyoxidizer_cache>`.

If you already have Rust installed on your machine and want PyOxidizer to
use the existing Rust installation, either pass the ``--system-rust`` flag
to ``pyoxidizer`` invocations or define the ``PYOXIDIZER_SYSTEM_RUST``
environment variable to any value. When the *system* Rust is being used,
``pyoxidizer`` will automatically use the ``cargo`` executable found
on the current search path (typically the ``PATH`` environment variable).

Creating New Projects with ``init-config-file``
===============================================

The ``pyoxidizer init-config-file`` command will create a new
``pyoxidizer.bzl`` configuration file in the target directory::

   $ pyoxidizer init-config-file pyapp

This should have printed out details on what happened and what to do next.

Creating New Rust Projects with ``init-rust-project``
=====================================================

The ``pyoxidizer init-rust-project`` command creates a minimal
Rust project configured to build an application that runs an
embedded Python interpreter from a configuration defined in a
``pyoxidizer.bzl`` configuration file. Run it by specifying the
directory to contain the new project::

   $ pyoxidizer init-rust-project pyapp

This should have printed out details on what happened and what to do next.

The explicit creation of Rust projects to use ``PyOxidizer`` is not
required. If your produced binaries only need to perform actions
configurable via ``PyOxidizer`` configuration files (like running
some Python code), an explicit Rust project isn't required, as
``PyOxidizer`` can auto-generate a temporary Rust project at build time.

But if you want to supplement the behavior of the binaries built
with Rust, an explicit and persisted Rust project can facilitate that.
For example, you may want to run custom Rust code before, during, and
after a Python interpreter runs in the process.

See :ref:`rust_projects` for more on the composition of Rust projects.

Building PyObject Projects with ``build``
=========================================

The ``pyoxidizer build`` command is probably the most important and used
``pyoxidizer`` command. This command evaluates a ``pyoxidizer.bzl``
configuration file by resolving *targets* in it.

By default, the default *target* in the configuration file is resolved.
However, callers can specify a list of explicit *targets* to resolve.
e.g.::

   # Resolve the default target.
   $ pyoxidizer build

   # Resolve the "exe" and "install" targets, in that order.
   $ pyoxidizer build exe install

``PyOxidizer`` configuration files are effectively defining a build
system, hence the name *build* for the command to resolve *targets*
within.

Running the Result of Building with ``run``
===========================================

Target functions in ``PyOxidizer`` configuration files return objects
that may be *runnable*. For example, a :py:class:`PythonExecutable`
returned by a target function that defines a Python executable binary
can be *run* by executing a new process.

The ``pyoxidizer run`` command is used to attempt to *run* an object
returned by a build target. It is effectively ``pyoxidizer build`` followed
by *running* the returned object. e.g.::

   # Run the default target.
   $ pyoxidizer run

   # Run the "install" target.
   $ pyoxidizer run --target install

Analyzing Produced Binaries with ``analyze``
============================================

The ``pyoxidizer analyze`` command is a generic command for analyzing the
contents of executables and libraries. While it is generic, its output is
specifically tailored for ``PyOxidizer``.

Run the command with the path to an executable. For example::

   $ pyoxidizer analyze build/apps/myapp/x86_64-unknown-linux-gnu/debug/myapp

Behavior is dependent on the format of the file being analyzed. But the
general theme is that the command attempts to identify the run-time
requirements for that binary. For example, for ELF binaries it will
list all shared library dependencies and analyze ``glibc`` symbol
versions and print out which Linux distributions it thinks the binary
is compatible with.

.. note::

   ``pyoxidizer analyze`` is not yet implemented for all executable
   file types that ``PyOxidizer`` supports.

Inspecting Python Distributions
===============================

``PyOxidizer`` uses special pre-built Python distributions to build
binaries containing Python.

These Python distributions are zstandard compressed tar files. Zstandard
is a modern compression format that is really, really, really good.
(PyOxidizer's maintainer also maintains
`Python bindings to zstandard <https://github.com/indygreg/python-zstandard>`_
and has
`written about the benefits of zstandard <https://gregoryszorc.com/blog/2017/03/07/better-compression-with-zstandard/>`_
on his blog. You should read that blog post so you are enlightened on
how amazing zstandard is.) But because zstandard is relatively new, not
all systems have utilities for decompressing that format yet. So, the
``pyoxidizer python-distribution-extract`` command can be used to extract
the zstandard compressed tar archive to a local filesystem path.

Python distributions contain software governed by a number of licenses.
This of course has implications for application distribution. See
:ref:`licensing_considerations` for more.

The ``pyoxidizer python-distribution-licenses`` command can be used to
inspect a Python distribution archive for information about its licenses.
The command will print information about the licensing of the Python
distribution itself along with a per-extension breakdown of which
libraries are used by which extensions and which licenses apply to what.
This command can be super useful to audit for license usage and only allow
extensions with licenses that you are legally comfortable with.

For example, the entry for the ``readline`` extension shows that the
extension links against the ``ncurses`` and ``readline`` libraries, which
are governed by the X11, and GPL-3.0 licenses::

   readline
   --------

   Dependency: ncurses
   Link Type: library

   Dependency: readline
   Link Type: library

   Licenses: GPL-3.0, X11
   License Info: https://spdx.org/licenses/GPL-3.0.html
   License Info: https://spdx.org/licenses/X11.html

.. note::

   The license annotations in Python distributions are best effort and
   can be wrong. They do not constitute a legal promise. Paranoid
   individuals may want to double check the license annotations by
   verifying with source code distributions, for example.

.. _cli_find_resources:

Debugging Resource Scanning and Identification with ``find-resources``
======================================================================

The ``pyoxidizer find-resources`` command can be used to scan for
resources in a given source and then print information on what's found.

PyOxidizer's packaging functionality scans directories and files and
classifies them as Python resources which can be operated on. See
:ref:`packaging_resource_types`. PyOxidizer's run-time importer/loader
(:ref:`oxidized_importer`) works by reading a pre-built index of known
resources. This all works in contrast to how Python typically works,
which is to put a bunch of files in directories and let the built-in
importer/loader figure it out by dynamically probing for various files.

Because PyOxidizer has introduced structure where it doesn't exist
in Python and because there are many subtle nuances with how files
are classified, there can be bugs in PyOxidizer's resource scanning
code.

The ``pyoxidizer find-resources`` command exists to facilitate
debugging PyOxidizer's resource scanning code.

Simply give the command a path to a directory or Python wheel archive
and it will tell you what it discovers. e.g.::

   $ pyoxidizer find-resources dist/oxidized_importer-0.1-cp38-cp38-manylinux1_x86_64.whl
   parsing dist/oxidized_importer-0.1-cp38-cp38-manylinux1_x86_64.whl as a wheel archive
   PythonExtensionModule { name: oxidized_importer }
   PythonPackageDistributionResource { package: oxidized-importer, version: 0.1, name: LICENSE }
   PythonPackageDistributionResource { package: oxidized-importer, version: 0.1, name: WHEEL }
   PythonPackageDistributionResource { package: oxidized-importer, version: 0.1, name: top_level.txt }
   PythonPackageDistributionResource { package: oxidized-importer, version: 0.1, name: METADATA }
   PythonPackageDistributionResource { package: oxidized-importer, version: 0.1, name: RECORD }

Or give it the path to a ``site-packages`` directory::

   $ pyoxidizer find-resources ~/.pyenv/versions/3.8.6/lib/python3.8/site-packages
   ...

This command needs to use a Python distribution so it knows what file
extensions correspond to Python extensions, etc. By default, it will
download one of the
:ref:`built-in distributions <packaging_python_distributions>` that is
compatible with the current machine and use that. You can specify a
``--distributions-dir`` to use to cache downloaded distributions::

   $ pyoxidizer find-resources --distributions-dir distributions /usr/lib/python3.8
   ...

.. _pyoxidizer_cli_extra_starlark_variables:

Defining Extra Variables in Starlark Environment
================================================

Various ``pyoxidizer`` commands (like ``build`` and ``run``) accept arguments
to define extra variables in the Starlark environment in the ``VARS``
global dict. This feature can be used to parameterize and conditionalize the
evaluation of configuration files.

.. note::

   While we could inject global variables into the Starlark environment,
   since it is illegal to access an undefined symbol (there's not even a
   way to test if a symbol is defined) and since we have no hook point to
   inject variables after the symbol has been defined, we resort to populating
   a global ``VARS`` dict with variables.

For example, let's make the name of the built executable dynamic:

.. code-block:: python

   DEFAULT_APP_NAME = "default"

   def make_exe(dist):
       dist = default_python_distribution()
       return dist.to_python_executable(name = VARS.get("app_name", DEFAULT_APP_NAME))

   register_target("exe", make_exe)

   resolve_targets()

Then let's build it::

   # Uses `default` as the application name.
   $ pyoxidizer build

   # Uses `my_app` as the application name.
   $ pyoxidizer build --var app_name my_app

   # Uses `env_name` as the application name via an environment variable.
   $ APP_NAME=env_name pyoxidizer build --var-env app_name APP_NAME
