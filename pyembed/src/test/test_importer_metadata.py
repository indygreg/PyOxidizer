# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import collections.abc
import email.message

# email.parser may be unused. However, it is needed by Rust code and some
# sys.path mucking in tests may prevent it from being imported. So import
# here to ensure it is cached in sys.modules so Rust can import it.
import email.parser
import importlib.metadata
import os
import pathlib
import sys
import tempfile
import unittest

from oxidized_importer import (
    OxidizedDistribution,
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
        self.old_finders = list(sys.meta_path)
        self.old_path = list(sys.path)

    def tearDown(self):
        self.raw_temp_dir.cleanup()
        del self.raw_temp_dir
        del self.td
        sys.meta_path[:] = self.old_finders
        sys.path[:] = self.old_path

    def _write_metadata(self):
        metadata_path = self.td / "my_package-1.0.dist-info" / "METADATA"
        metadata_path.parent.mkdir()

        with metadata_path.open("w", encoding="utf-8") as fh:
            fh.write("Name: my_package\n")
            fh.write("Version: 1.0\n")

    def _finder_from_td(self):
        collector = OxidizedResourceCollector(allowed_locations=["in-memory"])
        for r in find_resources_in_path(self.td):
            collector.add_in_memory(r)

        f = OxidizedFinder()
        f.add_resources(collector.oxidize()[0])

        return f

    def test_find_distributions_empty(self):
        f = OxidizedFinder()
        dists = f.find_distributions()
        self.assertIsInstance(dists, collections.abc.Iterator)
        dists = list(dists)
        self.assertEqual(len(dists), 0)

    def test_find_distributions_default_context(self):
        self._write_metadata()
        f = self._finder_from_td()

        dists = f.find_distributions(importlib.metadata.DistributionFinder.Context())
        self.assertIsInstance(dists, collections.abc.Iterator)
        dists = list(dists)
        self.assertEqual(len(dists), 1)

    def test_find_distributions_context_unknown_name(self):
        f = OxidizedFinder()

        dists = list(
            f.find_distributions(
                importlib.metadata.DistributionFinder.Context(name="missing")
            )
        )
        self.assertEqual(len(dists), 0)

    def test_find_distributions_context_name(self):
        self._write_metadata()
        f = self._finder_from_td()

        dists = list(
            f.find_distributions(
                importlib.metadata.DistributionFinder.Context(name="my_package")
            )
        )
        self.assertEqual(len(dists), 1)
        dist = dists[0]
        self.assertIsInstance(dist, OxidizedDistribution)
        self.assertEqual(dist.version, "1.0")

    def test_find_distributions_case_sensitivity(self):
        pkginfo_path = self.td / "OneTwo-1.0.egg-info" / "PKG-INFO"
        pkginfo_path.parent.mkdir()

        with pkginfo_path.open("w", encoding="utf-8") as fh:
            fh.write("Name: OneTwo\n")
            fh.write("Version: 1.0\n")

        f = self._finder_from_td()

        dists = list(
            f.find_distributions(
                importlib.metadata.DistributionFinder.Context(name="onetwo")
            )
        )
        self.assertEqual(len(dists), 1)

        dists = list(
            f.find_distributions(
                importlib.metadata.DistributionFinder.Context(name="OneTwo")
            )
        )
        self.assertEqual(len(dists), 1)

    def test_read_text(self):
        self._write_metadata()
        f = self._finder_from_td()

        dists = list(f.find_distributions())
        self.assertEqual(len(dists), 1)

        d = dists[0]

        self.assertIsInstance(d, OxidizedDistribution)

        # read_text() on missing file returns None.
        self.assertIsNone(d.read_text("does_not_exist"))

        data = d.read_text("METADATA")
        self.assertEqual(data, "Name: my_package\nVersion: 1.0\n")

    def test_load_metadata(self):
        self._write_metadata()
        f = self._finder_from_td()

        dists = list(f.find_distributions())
        self.assertEqual(len(dists), 1)

        metadata = dists[0].metadata
        self.assertIsInstance(metadata, email.message.Message)

        # On Python 3.10+, there is an adapter class.
        try:
            from importlib.metadata._adapters import Message as Adapter

            self.assertIsInstance(metadata, Adapter)
        except ImportError:
            pass

        self.assertEqual(metadata["Name"], "my_package")
        self.assertEqual(metadata["Version"], "1.0")

    def test_load_pkg_info(self):
        # In absence of a METADATA file, a PKG-INFO file will be read.
        pkginfo_path = self.td / "my_package-1.0.egg-info" / "PKG-INFO"
        pkginfo_path.parent.mkdir()

        with pkginfo_path.open("w", encoding="utf-8") as fh:
            fh.write("Name: my_package\n")
            fh.write("Version: 1.0\n")

        collector = OxidizedResourceCollector(allowed_locations=["in-memory"])
        for r in find_resources_in_path(self.td):
            collector.add_in_memory(r)

        f = OxidizedFinder()
        f.add_resources(collector.oxidize()[0])

        dists = list(f.find_distributions())
        self.assertEqual(len(dists), 1)

        metadata = dists[0].metadata
        self.assertIsInstance(metadata, email.message.Message)
        self.assertEqual(metadata["Name"], "my_package")
        self.assertEqual(metadata["Version"], "1.0")

    def test_name(self):
        self._write_metadata()
        f = self._finder_from_td()

        dists = list(f.find_distributions())
        self.assertEqual(dists[0].name, "my_package")

    def test_normalized_name(self):
        metadata_path = self.td / "my_package-1.0.dist-info" / "METADATA"
        metadata_path.parent.mkdir()

        with metadata_path.open("w", encoding="utf-8") as fh:
            fh.write("Name: my-package\n")
            fh.write("Version: 1.0\n")

        f = self._finder_from_td()

        dists = list(f.find_distributions())
        self.assertEqual(dists[0].name, "my-package")
        self.assertEqual(dists[0]._normalized_name, "my_package")

    def test_version(self):
        self._write_metadata()
        f = self._finder_from_td()

        dists = list(f.find_distributions())
        self.assertEqual(dists[0].version, "1.0")

    def test_missing_entry_points(self):
        self._write_metadata()
        f = self._finder_from_td()

        dists = list(f.find_distributions())
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

        dists = list(f.find_distributions())

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

        dists = list(f.find_distributions())

        self.assertIsNone(dists[0].requires)

    def test_requires_metadata(self):
        self._write_metadata()

        with (self.td / "my_package-1.0.dist-info" / "METADATA").open("ab") as fh:
            fh.write(b"Requires-Dist: foo\n")
            fh.write(b"Requires-Dist: bar; extra == 'all'\n")

        f = self._finder_from_td()
        dists = list(f.find_distributions())

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
        dists = list(f.find_distributions())
        self.assertEqual(len(dists), 1)

        requires = dists[0].requires
        self.assertIsInstance(requires, list)
        self.assertEqual(requires, ["foo"])

    def test_distribution_locate_file(self):
        self._write_metadata()
        f = self._finder_from_td()

        dist = list(f.find_distributions())[0]

        with self.assertRaises(AttributeError):
            dist.locate_file("METADATA")

    def test_distribution_from_name(self):
        self._write_metadata()
        f = self._finder_from_td()

        sys.meta_path = [f]
        sys.path = []

        with self.assertRaises(importlib.metadata.PackageNotFoundError):
            OxidizedDistribution.from_name("missing")

        dist = OxidizedDistribution.from_name("my_package")
        self.assertIsInstance(dist, OxidizedDistribution)

        metadata = importlib.metadata.metadata("my_package")
        self.assertIsInstance(metadata, email.message.Message)
        self.assertEqual(metadata["Name"], "my_package")
        self.assertEqual(metadata["Version"], "1.0")

    def test_distribution_discover(self):
        self._write_metadata()
        f = self._finder_from_td()

        sys.meta_path = [f]
        sys.path = []

        dists = OxidizedDistribution.discover()
        self.assertIsInstance(dists, collections.abc.Iterator)
        dists = list(dists)
        self.assertEqual(len(dists), 1)

        dist = dists[0]
        self.assertEqual(dist.metadata["Name"], "my_package")

    def test_distribution_discover_context_kwarg(self):
        self._write_metadata()
        f = self._finder_from_td()

        sys.meta_path = [f]
        sys.path = []

        dists = list(
            OxidizedDistribution.discover(
                context=importlib.metadata.DistributionFinder.Context(name="missing")
            )
        )
        self.assertEqual(len(dists), 0)

        dists = list(
            OxidizedDistribution.discover(
                context=importlib.metadata.DistributionFinder.Context()
            )
        )
        self.assertEqual(len(dists), 1)

        dists = list(
            OxidizedDistribution.discover(
                context=importlib.metadata.DistributionFinder.Context(name="my_package")
            )
        )
        self.assertEqual(len(dists), 1)

    def test_distribution_discover_name_kwarg(self):
        self._write_metadata()
        f = self._finder_from_td()

        sys.meta_path = [f]
        sys.path = []

        dists = list(OxidizedDistribution.discover(name="missing"))
        self.assertEqual(len(dists), 0)

        dists = list(OxidizedDistribution.discover(name="my_package"))
        self.assertEqual(len(dists), 1)

    def test_distribution_discover_conflicting_args(self):
        with self.assertRaises(ValueError):
            OxidizedDistribution.discover(context="ignored", name="ignored")

    def test_distribution_at(self):
        self._write_metadata()
        f = self._finder_from_td()

        sys.meta_path = [f]
        sys.path = []

        # Not yet implemented.
        with self.assertRaises(AttributeError):
            OxidizedDistribution.at(self.td)


if __name__ == "__main__":
    unittest.main()
