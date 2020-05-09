# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import pathlib
import sys
import tempfile
import unittest

from oxidized_importer import (
    OxidizedFinder,
    OxidizedResourceCollector,
    find_resources_in_path,
)


class TestImporterResourceReading(unittest.TestCase):
    def setUp(self):
        self.raw_temp_dir = tempfile.TemporaryDirectory(
            prefix="oxidized_importer-test-"
        )
        self.td = pathlib.Path(self.raw_temp_dir.name)

    def tearDown(self):
        self.raw_temp_dir.cleanup()
        del self.raw_temp_dir
        del self.td

    def _finder_from_td(self):
        collector = OxidizedResourceCollector(policy="in-memory-only")
        for r in find_resources_in_path(self.td):
            collector.add_in_memory(r)

        f = OxidizedFinder()
        f.add_resources(collector.oxidize()[0])

        return f

    def test_get_resource_reader_missing_package(self):
        f = self._finder_from_td()
        self.assertIsNone(f.get_resource_reader("my_package"))

    def test_get_resource_reader_not_package(self):
        with (self.td / "my_package.py").open("wb"):
            pass

        f = self._finder_from_td()

        self.assertIsNone(f.get_resource_reader("my_package"))


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
