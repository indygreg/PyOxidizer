# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import os
import pathlib
import sys
import tempfile
import unittest

from oxidized_importer import (
    OxidizedFinder,
    OxidizedResourceCollector,
    find_resources_in_path,
)


class TestImporterResourceScanning(unittest.TestCase):
    def setUp(self):
        self.raw_temp_dir = tempfile.TemporaryDirectory(
            prefix="oxidized_importer-test-"
        )
        self.td = pathlib.Path(self.raw_temp_dir.name)

    def tearDown(self):
        self.raw_temp_dir.cleanup()
        del self.raw_temp_dir
        del self.td

    def test_construct(self):
        with self.assertRaises(TypeError):
            OxidizedResourceCollector()

        c = OxidizedResourceCollector(policy="in-memory-only")
        self.assertEqual(c.policy, "in-memory-only")

    def test_source_module(self):
        c = OxidizedResourceCollector(policy="in-memory-only")

        source_path = self.td / "foo.py"

        with source_path.open("wb") as fh:
            fh.write(b"import io\n")

        for resource in find_resources_in_path(self.td):
            c.add_in_memory(resource)

        f = OxidizedFinder()
        f.add_resources(c.oxidize()[0])

        resources = [r for r in f.indexed_resources() if r.name == "foo"]
        self.assertEqual(len(resources), 1)

        r = resources[0]
        self.assertEqual(r.in_memory_source, b"import io\n")

    def test_add_sys_path(self):
        c = OxidizedResourceCollector(
            policy="prefer-in-memory-fallback-filesystem-relative:prefix"
        )

        for path in sys.path:
            if os.path.isdir(path):
                for resource in find_resources_in_path(path):
                    c.add_in_memory(resource)
                    c.add_filesystem_relative("", resource)

        resources, file_installs = c.oxidize()
        f = OxidizedFinder()
        f.add_resources(resources)
        f.serialize_indexed_resources()


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
