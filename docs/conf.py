import os
import re

HERE = os.path.dirname(__file__)
ROOT = os.path.dirname(HERE)

release = 'unknown'

with open(os.path.join(ROOT, 'pyoxidizer', 'Cargo.toml'), 'r') as fh:
    for line in fh:
        m = re.match('^version = \"([^"]+)"', line)
        if m:
            release = m.group(1)
            break


project = 'PyOxidizer'
copyright = '2019, Gregory Szorc'
author = 'Gregory Szorc'
extensions = []
templates_path = ['_templates']
exclude_patterns = ['_build', 'Thumbs.db', '.DS_Store']
html_theme = 'alabaster'
html_static_path = ['_static']
master_doc = 'index'
