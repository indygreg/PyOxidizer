# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import email.message
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

    def test_find_distributions_empty(self):
        f = OxidizedFinder()
        dists = f.find_distributions()
        self.assertIsInstance(dists, list)
        self.assertEqual(len(dists), 0)

    def test_read_text(self):
        metadata_path = self.td / "my_package-1.0.dist-info" / "METADATA"
        metadata_path.parent.mkdir()

        with metadata_path.open("w", encoding="utf-8") as fh:
            fh.write("Name: my_package\n")
            fh.write("Version: 1.0\n")

        collector = OxidizedResourceCollector(policy="in-memory-only")
        for r in find_resources_in_path(self.td):
            collector.add_in_memory(r)

        f = OxidizedFinder()
        f.add_resources(collector.oxidize()[0])

        dists = f.find_distributions()
        self.assertIsInstance(dists, list)
        self.assertEqual(len(dists), 1)

        d = dists[0]

        self.assertEqual(d.__class__.__name__, "OxidizedDistribution")

        # read_text() on missing file returns None.
        self.assertIsNone(d.read_text("does_not_exist"))

        data = d.read_text("METADATA")
        self.assertEqual(data, "Name: my_package\nVersion: 1.0\n")

    def test_load_metadata(self):
        metadata_path = self.td / "my_package-1.0.dist-info" / "METADATA"
        metadata_path.parent.mkdir()

        with metadata_path.open("w", encoding="utf-8") as fh:
            fh.write("Name: my_package\n")
            fh.write("Version: 1.0\n")

        collector = OxidizedResourceCollector(policy="in-memory-only")
        for r in find_resources_in_path(self.td):
            collector.add_in_memory(r)

        f = OxidizedFinder()
        f.add_resources(collector.oxidize()[0])

        dists = f.find_distributions()
        self.assertIsInstance(dists, list)
        self.assertEqual(len(dists), 1)

        metadata = dists[0].metadata
        self.assertIsInstance(metadata, email.message.Message)
        self.assertEqual(metadata["Name"], "my_package")
        self.assertEqual(metadata["Version"], "1.0")


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
