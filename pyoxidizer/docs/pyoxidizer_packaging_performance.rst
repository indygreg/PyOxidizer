.. _packaging_performance:

=============================
Performance of Built Binaries
=============================

Binaries built with PyOxidizer tend to run faster than those executing via
a normal ``python`` interpreter. There are a few reasons for this.

Resources Data Compiled Into Binary
===================================

Traditionally, when Python needs to ``import`` a module, it traverses
the entries on ``sys.path`` and queries the filesystem to see whether
a ``.pyc`` file, ``.py`` file, etc are available until it finds a
suitable file to provide the Python module data. If you trace the
system calls of a Python process (e.g. ``strace -f python3 ...``),
you will see tons of ``lstat()``, ``open()``, and ``read()`` calls
performing filesystem I/O.

While filesystems cache the data behind these I/O calls, every time
Python looks up data in a file the process needs to context switch
into the kernel and then pass data back to Python. Repeated thousands
of times - or even millions of times across hundreds or thousands of
process invocations - the few microseconds of overhead plus the
I/O overhead for a cache miss can add up to significant overhead!

When binaries are built with PyOxidizer, all available Python resources
are discovered at build time. An index of these resources along with
the raw resource data is packed - often into the executable itself -
and made available to PyOxidizer's
:ref:`custom importer <oxidized_importer>`. When PyOxidizer services an
``import`` statement, looking up a module is effectively looking up a key
in a dictionary: there is no explicit filesystem I/O to discover the
location of a resource.

PyOxidizer's packed resources data supports storing raw resource data
inline or as a reference via a filesystem path.

If inline storage is used, resources are effectively loaded from memory,
often using 0-copy. There is no explicit filesystem I/O. The only
filesystem I/O that can occur is indirect, as the operating system
pages a memory page on first access. But this all happens in the kernel
memory subsystem and is typically faster than going through a
functionally equivalent system call to access the filesystem.

If filesystem paths are stored, the only filesystem I/O we require
is to ``open()`` the file and ``read()`` its file descriptor: all
filesystem I/O to locate the backing file is skipped, along with the
overhead of any Python code performing this discovery.

We can attempt to isolate the effect of in-memory module imports by running
a Python script that attempts to import the entirety of the Python standard
library. This test is a bit contrived. But it is effective at demonstrating
the performance difference.

Using a stock ``python3.7`` executable and 2 ``PyOxidizer`` executables - one
configured to load the standard library from the filesystem using Python's
default importer and another from memory::

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

We see that the ``PyOxidizer`` executable using the standard Python importer
has very similar performance to ``python3.7``. But the ``PyOxidizer`` executable
importing from memory is clearly faster. These measurements were obtained
on macOS and the ``import_stdlib.py`` script imports 506 modules.

A less contrived example is running the test harness for the Mercurial version
control tool. Mercurial's test harness creates tens of thousands of new processes
that start Python interpreters. So a few milliseconds of overhead starting
interpreters or loading modules can translate to several seconds.

We run the full Mercurial test harness on Linux on a Ryzen 3950X CPU using the
following variants:

* ``hg`` script with a ``#!/path/to/python3.7`` line (traditional)
* ``hg`` PyOxidizer executable using Python's standard filesystem import (oxidized)
* ``hg`` PyOxidizer executable using *filesystem-relative* resource loading (filesystem)
* ``hg`` PyOxidizer executable using *in-memory* resource loading (in-memory)

The results are quite clear:

+-------------+--------------+-----------+--------+
| Variant     | CPU Time (s) | Delta (s) | % Orig |
+=============+==============+===========+========+
| traditional |       11,287 |         0 |    100 |
+-------------+--------------+-----------+--------+
| oxidized    |       10,735 |      -552 |   95.1 |
+-------------+--------------+-----------+--------+
| filesystem  |       10,186 |    -1,101 |   90.2 |
+-------------+--------------+-----------+--------+
| in-memory   |        9,883 |    -1,404 |   87.6 |
+-------------+--------------+-----------+--------+

These results help us isolate specific areas of speedups:

* *oxidized* over *traditional* is a rough proxy for the benefits of
  ``python -S`` over ``python``. Although there are other factors at
  play that may be influencing the numbers.
* *filesystem* over *oxidized* isolates the benefits of using PyOxidizer's
  importer instead of Python's default importer. The performance wins here
  are due to a) avoiding excessive I/O system calls to locate the paths
  to resources and b) functionality being implemented in Rust instead
  of Python.
* *in-memory* over *filesystem* isolates the benefits of avoiding
  explicit filesystem I/O to load Python resources. The Rust code
  backing these 2 variants is very similar. The only meaningful
  difference is that *in-memory* constructs a Python object from
  a memory address and *filesystem* must open and read a file using
  standard OS mechanisms before doing so.

From this data, one could draw a few conclusions:

* Processing of the ``site`` module during Python interpreter
  initialization can add substantial overhead.
* Maintaining an index of Python resources such that you can avoid
  discovery via filesystem I/O provides a meaningful speedup.
* Loading Python resources from an in-memory data structure is
  faster than incurring explicit filesystem I/O to do so.

Ignoring ``site``
=================

In its default configuration, binaries produced with PyOxidizer configure
the embedded Python interpreter differently from how a ``python`` is
typically configured.

Notably, PyOxidizer disables the importing of the ``site`` module by
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
