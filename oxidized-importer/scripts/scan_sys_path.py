#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Simple script to scan sys.path, for benchmarking purposes."""

import os
import sys

import oxidized_importer


USAGE = "Usage: scan_sys_path.py python|oxidized"


def scan_python(path):
    for root, dirs, files in os.walk(path):
        pass


def scan_oxidized(path):
    oxidized_importer.find_resources_in_path(path)


if len(sys.argv) != 2:
    print(USAGE)
    sys.exit(1)

if sys.argv[1] == "python":
    fn = scan_python
elif sys.argv[1] == "oxidized":
    fn = scan_oxidized
else:
    print(USAGE)
    sys.exit(1)

for path in sys.path:
    if os.path.isdir(path):
        print("scanning %s" % path)
        fn(path)
