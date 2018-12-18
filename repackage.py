# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import os
import pathlib
import re
import struct

STDLIB_TEST_DIRS = tuple(pathlib.PurePosixPath(p) for p in (
    'bsddb/test',
    'ctypes/test',
    'distutils/tests',
    'email/test',
    'idlelib/idle_test',
    'json/tests',
    'lib-tk/test',
    'lib2to3/tests',
    'sqlite3/test',
    'test',
    'tkinter/test',
    'unittest/test',
))

STDLIB_NONTEST_IGNORE_DIRS = tuple(pathlib.PurePosixPath(p) for p in (
    # The config directory describes how Python was built. It isn't relevant.
    'config',
    # ensurepip is useful for Python installs, which we're not. Ignore it.
    'ensurepip',
    # We don't care about the IDLE IDE.
    'idlelib',
    # lib2to3 is used for porting Python 2 to Python 3. While there may be some
    # useful generic functions in there for rewriting Python source, it is quite
    # large. So let's not include it.
    'lib2to3',
    # site-packages is where additional packages go. We don't use it.
    'site-packages',
))


# Files in Python standard library that should never be repacked.
STDLIB_IGNORE_FILES = tuple(pathlib.PurePosixPath(p) for p in (
    # These scripts are used for building macholib. They don't need to be in
    # the standard library.
    'ctypes/macholib/fetch_macholib',
    'ctypes/macholib/etch_macholib.bat',
    'ctypes/macholib/README.ctypes',
    'distutils/README',
    'wsgiref.egg-info',
))


def stdlib_path_relevant(p: pathlib.Path):
    """Whether a path in the standard library is relevant to repackaging.

    The passed path should be relative to the root of the standard library.
    e.g. from lib/pythonX.Y/.
    """
    # Compiled Python files are not the canonical source of data. So ignore
    # them here.
    ext = os.path.splitext(p)[1]

    if ext in ('.pyc', '.pyo'):
        return False

    # config-X.Y* directories describe how Python was built and are never
    # relevant.
    if re.match('config-\d\.\d', str(p)):
        return False

    # distutils/command contains some .exe files (even on Linux!). Those aren't
    # useful.
    if re.match('distutils/command/.*\.exe', str(p)):
        return False

    p = pathlib.PurePosixPath(p)

    if p in STDLIB_IGNORE_FILES:
        return False

    for ignore in STDLIB_TEST_DIRS + STDLIB_NONTEST_IGNORE_DIRS:
        try:
            p.relative_to(ignore)
            return False
        except ValueError:
            pass

    return True


def make_path_filter(ignore_stdlib_test_dirs=True, ignore_common_stdlib_files=True,
                     ignore_common_stdlib_dirs=True):
    def match(p: pathlib.PurePosixPath):
        ext = os.path.splitext(p)[1]

        if ext in ('.pyc', '.pyo'):
            return False

        if ignore_common_stdlib_files and p in STDLIB_IGNORE_FILES:
            return False

        if ignore_stdlib_test_dirs:
            for ignore in STDLIB_TEST_DIRS:
                try:
                    p.relative_to(ignore)
                    return False
                except ValueError:
                    pass

        if ignore_common_stdlib_dirs:
            for ignore in STDLIB_NONTEST_IGNORE_DIRS:
                try:
                    p.relative_to(ignore)
                    return False
                except ValueError:
                    pass

        return True

    return match
