#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import os
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(os.path.abspath(os.path.dirname(__file__))).parent
PYEMBED_PATH = ROOT / "pyembed"

PYTHON_EXE = pathlib.Path(sys.executable)
PYTHON_DIR = PYTHON_EXE.parent

os.environ["PYTHON_SYS_EXECUTABLE"] = str(PYTHON_EXE)

if os.name == "nt":
    os.environ["PATH"] = "%s;%s" % (PYTHON_DIR, os.environ["PATH"])
else:
    os.environ["PATH"] = "%s:%s" % (PYTHON_DIR, os.environ["PATH"])

    ld_path = os.environ.get("LD_LIBRARY_PATH")

    lib_path = PYTHON_DIR.parent / "lib"

    if ld_path:
        os.environ["LD_LIBRARY_PATH"] = "%s:%s" % (lib_path, ld_path)
    else:
        os.environ["LD_LIBRARY_PATH"] = str(lib_path)

sys.exit(
    subprocess.run(
        ["cargo", "test", "-p", "pyembed"], cwd=str(ROOT), env=os.environ
    ).returncode
)
