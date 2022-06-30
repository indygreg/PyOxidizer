# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import os
import pathlib
import re

HERE = pathlib.Path(os.path.dirname(__file__))
ROOT = pathlib.Path(os.path.dirname(HERE))

release = "unknown"

with (ROOT / "Cargo.toml").open("r") as fh:
    for line in fh:
        m = re.match('^version = "([^"]+)"', line)
        if m:
            release = m.group(1)
            break


project = "Apple Codesign"
copyright = "2022, Gregory Szorc"
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
tags.add("apple_codesign")
