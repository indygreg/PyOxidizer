.. _overview:

========
Overview
========

From a very high level, ``PyOxidizer`` is a tool for packaging and
distributing Python applications. The over-arching goal of ``PyOxidizer``
is to make this (often complex) problem space simple so application
maintainers can focus on building quality applications instead of
toiling with build systems and packaging tools.

On a lower, more technical level, ``PyOxidizer`` has a command line
tool - ``pyoxidizer`` - that is capable of building binaries (executables
or libraries) that embed a fully-functional Python interpreter plus
Python extensions and modules *in a single binary*. Binaries produced
with ``PyOxidizer`` are highly portable and can work on nearly every
system without any special requirements like containers, FUSE filesystems,
or even temporary directory access. On Linux, ``PyOxidizer`` can
produce executables that are fully statically linked and don't even
support dynamic loading.

The *Oxidizer* part of the name comes from Rust: binaries built with
``PyOxidizer`` are compiled from Rust and Rust code is responsible for
managing the embedded Python interpreter and all its operations. But the
existence of Rust should be invisible to many users, much like the fact
that CPython (the official Python distribution available from www.python.org)
is implemented in C. Rust is simply a tool to achieve an end goal (albeit
a rather effective and powerful tool).

Benefits of PyOxidizer
======================

You may be wondering why you should use or care about ``PyOxidizer``.
Great question!

Python application distribution is generally considered an unsolved
problem. At PyCon 2019, Russel Keith-Magee
`identified code distribution <https://youtu.be/ftP5BQh1-YM?t=2033>`_ as
a potential *black swan* for Python during a keynote talk. In their words,
*Python hasn't ever had a consistent story for how I give my code to someone
else, especially if that someone else isn't a developer and just wants to
use my application.* The over-arching goal of ``PyOxidizer`` is to solve this
problem. If we're successful, we help Python become a more attractive
option in more domains and eliminate this potential *black swan* that
is an existential threat for Python's longevity.

On a less existential level, there are several benefits to ``PyOxidizer``.

Ease of Application Installation
--------------------------------

Installing Python applications can be hard, especially if you aren't a
developer.

Applications produced with ``PyOxidizer`` are self-contained - as small as
a single file executable. From the perspective of the end-user, they get
an executable containing an application that *just works*. There's no need
to install a Python distribution on their system. There's no need to
muck with installing Python packages. There's no need to configure a
container runtime like Docker. There's just an executable containing an
embedded Python interpreter and associated Python application code and
running that executable *just works*. From the perspective of the end-user,
your application is just another platform native executable.

Ease of Packaging and Distribution
----------------------------------

Python application developers can spend a large amount of time
managing how their applications are packaged and distributed. There's
no universal standard for distributing Python applications. Instead, there's
a hodgepodge of random tools, typically different tools per operating
system.

Python application developers typically need to *solve* the packaging
and distribution problem N times. This is thankless work and sucks valuable
time away from what could otherwise be spent improving the application
itself. Furthermore, each distinct Python application tends to solve this
problem redundantly.

Again, the over-arching goal of ``PyOxidizer`` is to provide a comprehensive
solution to the Python application packaging and distribution problem space.
We want to make it as turn-key as possible for application maintainers to
make their applications usable by novice computer users. If we're successful,
Python developers can spend less time solving packaging and distribution
problems and more time improving Python applications themselves. That's
good for the Python ecosystem.

.. _better_performance:

Faster Python Programs
----------------------

Binaries built with ``PyOxidizer`` tend to run faster than those
executing via a normal ``python`` interpreter. There are a few reasons
for this.

In its default configuration, binaries produced with ``PyOxidizer`` configure
the embedded Python interpreter differently from how a ``python`` is
typically configured.

Notably, ``PyOxidizer`` disables the importing of the ``site`` module by
default (making it roughly equivalent to ``python -S``). The ``site`` module
does a number of things, such as look for ``.pth`` files, looks for
``site-packages`` directories, etc. These activities can contribute
substantial overhead, as measured through a normal ``python3.7`` executable
on macOS::

   $ hyperfine -m 500 -- '/usr/local/bin/python3.7 -c 1' '/usr/local/bin/python3.7 -S -c 1'
   Benchmark #1: /usr/local/bin/python3.7 -c 1
     Time (mean ± σ):      22.7 ms ±   2.0 ms    [User: 16.7 ms, System: 4.2 ms]
     Range (min … max):    18.4 ms …  32.7 ms    500 runs

   Benchmark #2: /usr/local/bin/python3.7 -S -c 1
     Time (mean ± σ):      12.7 ms ±   1.1 ms    [User: 8.2 ms, System: 2.9 ms]
     Range (min … max):     9.8 ms …  16.9 ms    500 runs

   Summary
     '/usr/local/bin/python3.7 -S -c 1' ran
       1.78 ± 0.22 times faster than '/usr/local/bin/python3.7 -c 1'

Shaving ~10ms off of startup overhead is not trivial!

Another performance benefit comes from importing modules from memory.
``PyOxidizer`` supports importing Python modules from memory using
zero-copy. Traditionally, Python performs filesystem I/O to find and load
modules. Filesystem I/O performance is intrinsically inconsistent and depends
on several factors. Importing modules from memory removes the filesystem
API overhead and the only inconsistency is whether the binary's memory
address range containing Python modules is paged in. (On first binary load
the first access of a memory address will require the underling file to be
paged in by the kernel. But this all happens in the kernel and avoids
filesystem API overhead from userland.)

We can attempt to isolate the effect of in-memory module imports by running
a Python script that attempts to import the entirety of the Python standard
library. This test is a bit contrived. But it is effective at demonstrating
the performance difference.

Using a stock ``python3.7`` executable and 2 ``PyOxidizer`` executables - one
configured to load the standard library from the filesystem and another from
memory::

   $ hyperfine -m 50 -- '/usr/local/bin/python3.7 -S import_stdlib.py' import-stdlib-filesystem import-stdlib-memory
   Benchmark #1: /usr/local/bin/python3.7 -S import_stdlib.py
     Time (mean ± σ):     258.8 ms ±   8.9 ms    [User: 220.2 ms, System: 34.4 ms]
     Range (min … max):   247.7 ms … 310.5 ms    50 runs

   Benchmark #2: import-stdlib-filesystem
     Time (mean ± σ):     249.4 ms ±   3.7 ms    [User: 216.3 ms, System: 29.8 ms]
     Range (min … max):   243.5 ms … 258.5 ms    50 runs

   Benchmark #3: import-stdlib-memory
     Time (mean ± σ):     217.6 ms ±   6.4 ms    [User: 200.4 ms, System: 13.7 ms]
     Range (min … max):   207.9 ms … 243.1 ms    50 runs

   Summary
     'import-stdlib-memory' ran
       1.15 ± 0.04 times faster than 'import-stdlib-filesystem'
       1.19 ± 0.05 times faster than '/usr/local/bin/python3.7 -S import_stdlib.py'

We see that the ``PyOxidizer`` executable importing from the filesystem has
very similar performance to ``python3.7``. But the ``PyOxidizer`` executable
importing from memory is clearly faster. These measurements were obtained
on macOS and the ``import_stdlib.py`` script imports 506 modules.

.. _components:

Components
==========

The most visible component of ``PyOxidizer`` is the ``pyoxidizer`` command
line tool. This tool contains functionality for creating new projects using
``PyOxidizer``, adding ``PyOxidizer`` to existing projects, producing
binaries containing a Python interpreter, and various related functionality.

The ``pyoxidizer`` executable is written in Rust. Behind that tool is a pile
of Rust code performing all the functionality exposed by the tool. That code
is conveniently also made available as a library, so anyone wanting to
integrate ``PyOxidizer``'s core functionality without using our ``pyoxidizer``
tool is able to do so.

The ``pyoxidizer`` crate and command line tool are effectively glorified build
tools: they simply help with various project management, build, and packaging.

The run-time component of ``PyOxidizer`` is completely separate from the
build-time component. The run-time component of ``PyOxidizer`` consists of a
Rust crate named ``pyembed``. The role of the ``pyembed`` crate is to manage an
embedded Python interpreter. This crate contains all the code needed to
interact with the CPython APIs to create and run a Python interpreter.
``pyembed`` also contains the special functionality required to import
Python modules from memory using zero-copy.

How It Works
============

The ``pyoxidizer`` tool is used to create a new project or add ``PyOxidizer``
to an existing (Rust) project. This entails:

* Adding a copy of the ``pyembed`` crate to the project.
* Generating a boilerplate Rust source file to call into the ``pyembed`` crate
  to run a Python interpreter.
* Generating a working ``pyoxidizer.toml`` :ref:`configuration file <config_files>`.
* Telling the project's Rust build system about ``PyOxidizer``.

When that project's ``pyembed`` crate is built by Rust's build system, it calls
out to ``PyOxidizer`` to process the active ``PyOxidizer`` configuration file.
``PyOxidizer`` will obtain a specially-built Python distribution that is
optimized for embedding. It will then use this distribution to finish packaging
itself and any other Python dependencies indicated in the configuration file.
For example, you can process a pip requirements file at build time to include
additional Python packages in the produced binary.

At the end of this sausage grinder, ``PyOxidizer`` emits an archive library
containing Python (which can be linked into another library or executable)
and *resource files* containing Python data (such as Python module sources and
bytecode). Most importantly, ``PyOxidizer`` tells Rust's build system how to
integrate these components into the binary it is building.

From here, Rust's build system combines the standard Rust bits with the
files produced by ``PyOxidizer`` and turns everything into a binary,
typically an executable.

At run time, an instance of the ``PythonConfig`` struct from the ``pyembed``
crate is created to define how an embedded Python interpreter should behave.
(One of the build-time actions performed by ``PyOxidizer`` is to convert the
TOML configuration file into a default instance of this struct.) This struct
is used to instantiate a Python interpreter.

The ``pyembed`` crate implements a Python *extension module* which provides
custom module importing functionality. Light magic is used to coerce the
Python interpreter to load this module very early during initialization.
This allows the module to service Python ``import`` requests. The custom module
importer installed by ``pyembed`` supports retrieving data from a read-only
data structure embedded in the executable itself. Essentially, the Python
``import`` request calls into some Rust code provided by ``pyembed`` and
Rust returns a ``void *`` to memory containing data (module source code,
bytecode, etc) that was generated at build time by ``PyOxidizer`` and later
embedded into the binary by Rust's build system.

Once the embedded Python interpreter is initialized, the application works
just like any other Python application! The main differences are that modules
are (probably) getting imported from memory and that Rust - not the Python
distribution's ``python`` executable logic - is driving execution of Python.

Read on to :ref:`getting_started` to learn how to use ``PyOxidizer``.
