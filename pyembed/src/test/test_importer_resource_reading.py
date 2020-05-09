# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import io
import pathlib
import sys
import tempfile
import unittest

from oxidized_importer import (
    OxidizedFinder,
    OxidizedResourceCollector,
    OxidizedResourceReader,
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

    def _make_package(self, name):
        package_path = self.td

        for part in name.split("."):
            package_path = package_path / part

            package_path.mkdir(exist_ok=True)

            with (package_path / "__init__.py").open("wb"):
                pass

        return package_path

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

    def test_top_level_package(self):
        p = self._make_package("my_package")

        with (p / "resource.txt").open("wb") as fh:
            fh.write(b"my resource")

        f = self._finder_from_td()

        entries = [r for r in f.indexed_resources() if r.name == "my_package"]
        self.assertEqual(len(entries), 1)
        self.assertTrue(entries[0].is_package)

        r = f.get_resource_reader("my_package")

        self.assertIsInstance(r, OxidizedResourceReader)

        with self.assertRaises(FileNotFoundError):
            r.is_resource("missing")

        self.assertTrue(r.is_resource("resource.txt"))

        contents = r.contents()
        self.assertIsInstance(contents, list)
        self.assertEqual(contents, ["resource.txt"])

        with self.assertRaises(FileNotFoundError):
            r.resource_path("resource.txt")

        with self.assertRaises(FileNotFoundError):
            r.open_resource("missing")

        f = r.open_resource("resource.txt")
        self.assertIsInstance(f, io.BytesIO)
        self.assertEqual(f.getvalue(), b"my resource")

    def test_child_directory(self):
        p = self._make_package("my_package")

        child0_path = p / "child0"
        child1_path = p / "child1"

        child0_path.mkdir()
        child1_path.mkdir()

        with (child0_path / "a.txt").open("wb") as fh:
            fh.write(b"a")
        with (child1_path / "b.txt").open("wb") as fh:
            fh.write(b"b")

        f = self._finder_from_td()
        r = f.get_resource_reader("my_package")

        self.assertIsInstance(r, OxidizedResourceReader)

        self.assertTrue(r.is_resource("child0/a.txt"))
        self.assertTrue(r.is_resource("child1/b.txt"))

        self.assertEqual(r.contents(), ["child0/a.txt", "child1/b.txt"])

        self.assertEqual(r.open_resource("child0/a.txt").getvalue(), b"a")
        self.assertEqual(r.open_resource("child1/b.txt").getvalue(), b"b")


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
