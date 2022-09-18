# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
import os
import pathlib
import re

EXTERNAL_PREFIXES = (
    "oxidized_importer",
    "pyembed",
    "pyoxidizer",
    "pyoxy",
    "tugger",
)

EXTERNAL_SOURCE_DIRS = (
    pathlib.Path("pyembed") / "docs",
    pathlib.Path("pyoxidizer") / "docs",
    pathlib.Path("pyoxy") / "docs",
    pathlib.Path("python-oxidized-importer") / "docs",
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
copyright = "2019-present, Gregory Szorc"
author = "Gregory Szorc"
extensions = ["sphinx.ext.intersphinx"]
templates_path = ["_templates"]
exclude_patterns = ["_build", "Thumbs.db", ".DS_Store"]
html_theme = "alabaster"
master_doc = "index"
intersphinx_mapping = {
    "python": ("https://docs.python.org/3", None),
    "setuptools": ("https://setuptools.pypa.io/en/latest", None),
}
tags.add("global")

# Synchronize external docs into this directory.

# Start by collecting the set of external docs and their content.
# We'll use this to compute a minimal mutation so incremental Sphinx
# rebuilds are faster.
wanted_external_files = {}
for d in EXTERNAL_SOURCE_DIRS:
    source_dir = ROOT / d

    for f in os.listdir(source_dir):
        if not f.endswith(("rst", "png")) or not f.startswith(EXTERNAL_PREFIXES):
            continue

        source_path = source_dir / f
        dest_path = HERE / f

        with source_path.open("rb") as fh:
            source_data = fh.read()

        wanted_external_files[dest_path] = source_data


for f in sorted(os.listdir(HERE)):
    path = HERE / f

    if not f.startswith(EXTERNAL_PREFIXES):
        continue

    if path in wanted_external_files:
        with path.open("rb") as fh:
            current_data = fh.read()

        if current_data == wanted_external_files[path]:
            print("%s is up to date" % path)
        else:
            print("updating %s" % path)
            with path.open("wb") as fh:
                fh.write(wanted_external_files[path])

        del wanted_external_files[path]
    else:
        print("deleting %s since it disappeared" % path)
        path.unlink()

for path, data in sorted(wanted_external_files.items()):
    print("creating %s" % path)
    with path.open("wb") as fh:
        fh.write(data)
