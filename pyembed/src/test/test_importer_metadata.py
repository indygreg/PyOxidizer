# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import email.message
import importlib.metadata
import pathlib
import sys
import tempfile
import unittest

from oxidized_importer import (
    OxidizedFinder,
    OxidizedResourceCollector,
    find_resources_in_path,
)


class TestImporterMetadata(unittest.TestCase):
    def setUp(self):
        self.raw_temp_dir = tempfile.TemporaryDirectory(
            prefix="oxidized_importer-test-"
        )
        self.td = pathlib.Path(self.raw_temp_dir.name)

    def tearDown(self):
        self.raw_temp_dir.cleanup()
        del self.raw_temp_dir
        del self.td

    def _write_metadata(self):
        metadata_path = self.td / "my_package-1.0.dist-info" / "METADATA"
        metadata_path.parent.mkdir()

        with metadata_path.open("w", encoding="utf-8") as fh:
            fh.write("Name: my_package\n")
            fh.write("Version: 1.0\n")

    def _finder_from_td(self):
        collector = OxidizedResourceCollector(policy="in-memory-only")
        for r in find_resources_in_path(self.td):
            collector.add_in_memory(r)

        f = OxidizedFinder()
        f.add_resources(collector.oxidize()[0])

        return f

    def test_find_distributions_empty(self):
        f = OxidizedFinder()
        dists = f.find_distributions()
        self.assertIsInstance(dists, list)
        self.assertEqual(len(dists), 0)

    def test_read_text(self):
        self._write_metadata()
        f = self._finder_from_td()

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
        self._write_metadata()
        f = self._finder_from_td()

        dists = f.find_distributions()
        self.assertIsInstance(dists, list)
        self.assertEqual(len(dists), 1)

        metadata = dists[0].metadata
        self.assertIsInstance(metadata, email.message.Message)
        self.assertEqual(metadata["Name"], "my_package")
        self.assertEqual(metadata["Version"], "1.0")

    def test_load_pkg_info(self):
        # In absence of a METADATA file, a PKG-INFO file will be read.
        pkginfo_path = self.td / "my_package-1.0.egg-info" / "PKG-INFO"
        pkginfo_path.parent.mkdir()

        with pkginfo_path.open("w", encoding="utf-8") as fh:
            fh.write("Name: my_package\n")
            fh.write("Version: 1.0\n")

        collector = OxidizedResourceCollector(policy="in-memory-only")
        for r in find_resources_in_path(self.td):
            collector.add_in_memory(r)

        f = OxidizedFinder()
        f.add_resources(collector.oxidize()[0])

        dists = f.find_distributions()
        self.assertEqual(len(dists), 1)

        metadata = dists[0].metadata
        self.assertIsInstance(metadata, email.message.Message)
        self.assertEqual(metadata["Name"], "my_package")
        self.assertEqual(metadata["Version"], "1.0")

    def test_version(self):
        self._write_metadata()
        f = self._finder_from_td()

        dists = f.find_distributions()
        self.assertEqual(dists[0].version, "1.0")

    def test_missing_entry_points(self):
        self._write_metadata()
        f = self._finder_from_td()

        dists = f.find_distributions()
        self.assertEqual(len(dists), 1)

        eps = dists[0].entry_points

        # This is kinda weird but it is what the stdlib does when it receives None.
        self.assertIsInstance(eps, list)
        self.assertEqual(len(eps), 0)

    def test_populated_entry_points(self):
        self._write_metadata()

        entry_points_path = self.td / "my_package-1.0.dist-info" / "entry_points.txt"
        with entry_points_path.open("w", encoding="utf-8") as fh:
            fh.write("[console_scripts]\n")
            fh.write("script = my_package:module\n")

        f = self._finder_from_td()

        dists = f.find_distributions()

        eps = dists[0].entry_points

        self.assertIsInstance(eps, list)
        self.assertEqual(len(eps), 1)

        ep = eps[0]
        self.assertIsInstance(ep, importlib.metadata.EntryPoint)

        self.assertEqual(ep.name, "script")
        self.assertEqual(ep.value, "my_package:module")
        self.assertEqual(ep.group, "console_scripts")

    def test_requires_missing(self):
        self._write_metadata()
        f = self._finder_from_td()

        dists = f.find_distributions()

        self.assertIsNone(dists[0].requires)

    def test_requires_metadata(self):
        self._write_metadata()

        with (self.td / "my_package-1.0.dist-info" / "METADATA").open("ab") as fh:
            fh.write(b"Requires-Dist: foo\n")
            fh.write(b"Requires-Dist: bar; extra == 'all'\n")

        f = self._finder_from_td()
        dists = f.find_distributions()

        requires = dists[0].requires
        self.assertIsInstance(requires, list)
        self.assertEqual(requires, ["foo", "bar; extra == 'all'"])

    def test_requires_egg_info(self):
        pkginfo_path = self.td / "my_package-1.0.egg-info" / "PKG-INFO"
        pkginfo_path.parent.mkdir()

        with pkginfo_path.open("w", encoding="utf-8") as fh:
            fh.write("Name: my_package\n")
            fh.write("Version: 1.0\n")

        requires_path = self.td / "my_package-1.0.egg-info" / "requires.txt"
        with requires_path.open("w", encoding="utf-8") as fh:
            fh.write("foo\n")

        f = self._finder_from_td()
        dists = f.find_distributions()
        self.assertEqual(len(dists), 1)

        requires = dists[0].requires
        self.assertIsInstance(requires, list)
        self.assertEqual(requires, ["foo"])


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
