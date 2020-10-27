# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
import os
import pathlib
import re
import shutil

EXTERNAL_PREFIXES = ("oxidized_importer", "tugger")

EXTERNAL_SOURCE_DIRS = (
    pathlib.Path("pyembed") / "docs",
    pathlib.Path("tugger") / "docs",
)

HERE = pathlib.Path(os.path.dirname(__file__))
ROOT = pathlib.Path(os.path.dirname(HERE))

release = "unknown"

with (ROOT / "pyoxidizer" / "Cargo.toml").open("r") as fh:
    for line in fh:
        m = re.match('^version = "([^"]+)"', line)
        if m:
            release = m.group(1)
            break


project = "PyOxidizer"
copyright = "2019, Gregory Szorc"
author = "Gregory Szorc"
extensions = []
templates_path = ["_templates"]
exclude_patterns = ["_build", "Thumbs.db", ".DS_Store"]
html_theme = "alabaster"
master_doc = "index"

# Synchronize external docs into this directory.
for f in sorted(os.listdir(HERE)):
    if f.startswith(EXTERNAL_PREFIXES):
        print("deleting %s" % f)
        p = HERE / f
        p.unlink()

for d in EXTERNAL_SOURCE_DIRS:
    source_dir = ROOT / d

    for f in sorted(os.listdir(source_dir)):
        if f.endswith(".rst") and f.startswith(EXTERNAL_PREFIXES):
            source_path = source_dir / f
            dest_path = HERE / f
            print("copying %s to %s" % (source_path, dest_path))
            shutil.copyfile(source_path, dest_path)
