.. _terminfo_database:

=================
Terminfo Database
=================

.. note:: This content is not relevant to Windows.

If your application interacts with terminals (e.g. command line tools), your
application may require the availability of a ``terminfo`` database so your
application can properly interact with the terminal. The absence of a terminal
database can result in the inability to properly colorize text, the backspace
and arrow keys not working as expected, weird behavior on window resizing, etc.
A ``terminfo`` database is also required to use ``curses`` or ``readline``
module functionality without issue.

UNIX like systems almost always provide a ``terminfo`` database which says
which features and properties various terminals have. Essentially, the
``TERM`` environment variable defines the current terminal [emulator] in
use and the ``terminfo`` database converts that value to various settings.

From Python, the ``ncurses`` library is responsible for consulting the
``terminfo`` database and determining how to interact with the terminal.
This interaction with the ``ncurses`` library is typically performed from
the ``_curses``, ``_curses_panel``, and ``_readline`` C extensions. These
C extensions are wrapped by the user-facing ``curses`` and ``readline``
Python modules. And these Python modules can be used from various
functionality in the Python standard library. For example, the ``readline``
module is used to power ``pdb``.

**PyOxidizer applications do not ship a terminfo database.** Instead,
applications rely on the ``terminfo`` database on the executing machine.
(Of course, individual applications could ship a ``terminfo`` database if
they want: the functionality just isn't included in PyOxidizer by default.)
The reason PyOxidizer doesn't ship a ``terminfo`` database is that terminal
configurations are very system and user specific: PyOxidizer wants to
respect the configuration of the environment in which applications run. The
best way to do this is to use the ``terminfo`` database on the executing
machine instead of providing a static database that may not be properly
configured for the run-time environment.

PyOxidizer applications have the choice of various modes for resolving
the ``terminfo`` database location. This is facilitated mainly via the
:ref:`terminfo_resolution <config_terminfo_resolution>`
``PythonInterpreterConfig`` config setting.

By default, when Python is initialized PyOxidizer will try to identify
the current operating system and choose an appropriate set of well-known
paths for that operating system. If the operating system is well-known
(such as a Debian-based Linux distribution), this set of paths is fixed.
If the operating system is not well-known, PyOxidizer will look for
``terminfo`` databases at common paths and use whatever paths are
present.

If all goes according to plan, the default behavior *just works*. On
common operating systems, the cost to the default behavior is reading
a single file from the filesystem (in order to resolve the operating
system). The overhead should be negligible. For unknown operating
systems, PyOxidizer may need to ``stat()`` ~10 paths looking for the
``terminfo`` database. This should also complete fairly quickly. If
the overhead is a concern for you, it is recommended to build applications
with a fixed path to the ``terminfo`` database.

Under the hood, when PyOxidizer resolves the ``terminfo`` database
location, it communicates these paths to ``ncurses`` by setting the
``TERMINFO_DIRS`` environment variable. If the ``TERMINFO_DIRS``
environment variable is already set at application run-time, PyOxidizer
will **never** overwrite it.

The ``ncurses`` library that PyOxidizer applications ship with is also
configured to look for a ``terminfo`` database in the current user's
home directory (``HOME`` environment variable) by default, specifically
``$HOME/.terminfo``). Support for ``termcap`` databases is not enabled.

.. note::

   ``terminfo`` database behavior is intrinsically complicated because
   various operating systems do things differently. If you notice oddities
   in the interaction of PyOxidizer applications with terminals, there's
   a good chance you found a deficiency in PyOxidizer's terminal detection
   logic (which is located in the ``pyembed::osutils`` Rust module).

   Please report terminal interaction issues at
   https://github.com/indygreg/PyOxidizer/issues.
