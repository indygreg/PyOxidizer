# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# email.parser may be unused. However, it is needed by Rust code and some
# sys.path mucking in tests may prevent it from being imported. So import
# here to ensure it is cached in sys.modules so Rust can import it.
import email.parser

import os
import pathlib
import sys
import tempfile
import unittest

try:
    import pkg_resources
except ImportError:
    pkg_resources = None

from oxidized_importer import (
    OxidizedFinder,
    OxidizedPkgResourcesProvider,
    OxidizedResourceCollector,
    find_resources_in_path,
    register_pkg_resources,
)


@unittest.skipIf(pkg_resources is None, "pkg_resources not available")
class TestImporterPkgResources(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        register_pkg_resources()

    def setUp(self):
        self.raw_temp_dir = tempfile.TemporaryDirectory(
            prefix="oxidized_importer-test-"
        )
        self.td = pathlib.Path(self.raw_temp_dir.name)
        self.old_finders = list(sys.meta_path)
        self.old_path = list(sys.path)
        self.old_modules = dict(sys.modules)
        self.old_provider_factories = dict(pkg_resources._provider_factories)

    def tearDown(self):
        self.raw_temp_dir.cleanup()
        del self.raw_temp_dir
        del self.td
        sys.meta_path[:] = self.old_finders
        sys.path[:] = self.old_path
        sys.modules.clear()
        sys.modules.update(self.old_modules)
        pkg_resources._provider_factories.clear()
        pkg_resources._provider_factories.update(self.old_provider_factories)

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
        f.add_resources(
            collector.oxidize(python_exe=os.environ.get("PYTHON_SYS_EXECUTABLE"))[0]
        )

        return f

    def test_provider_registered(self):
        # Should have been done via setUpClass.
        self.assertEqual(
            pkg_resources._provider_factories.get(OxidizedFinder),
            OxidizedPkgResourcesProvider,
        )

    def test_package_provider(self):
        self._write_metadata()

        dist_info_path = self.td / "my_package-1.0.dist-info"
        (dist_info_path / "subdir").mkdir()
        (dist_info_path / "subdir" / "grandchild").mkdir()

        with (dist_info_path / "subdir" / "file.txt").open("wb"):
            pass

        with (dist_info_path / "subdir" / "grandchild" / "file2.txt").open("wb"):
            pass

        my_package_path = self.td / "my_package"
        my_package_path.mkdir(parents=True)

        with (my_package_path / "__init__.py").open("wb"):
            pass

        f = self._finder_from_td()
        sys.meta_path.insert(0, f)

        provider = pkg_resources.get_provider("my_package")
        self.assertIsInstance(provider, OxidizedPkgResourcesProvider)

        self.assertFalse(provider.has_metadata("foo"))
        self.assertTrue(provider.has_metadata("METADATA"))

        with self.assertRaises(IOError):
            provider.get_metadata("foo")

        self.assertEqual(
            provider.get_metadata("METADATA"), "Name: my_package\nVersion: 1.0\n"
        )

        lines = provider.get_metadata_lines("METADATA")
        self.assertEqual(next(lines), "Name: my_package")
        self.assertEqual(next(lines), "Version: 1.0")
        with self.assertRaises(StopIteration):
            next(lines)

        self.assertFalse(provider.metadata_isdir("foo"))
        self.assertFalse(provider.metadata_isdir("METADATA"))
        self.assertFalse(provider.metadata_isdir(""))
        self.assertTrue(provider.metadata_isdir("subdir"))
        self.assertTrue(provider.metadata_isdir("subdir/"))
        self.assertTrue(provider.metadata_isdir("subdir\\"))

        self.assertEqual(provider.metadata_listdir("missing"), [])
        self.assertEqual(provider.metadata_listdir(""), ["METADATA"])
        self.assertEqual(provider.metadata_listdir("subdir"), ["file.txt"])
        self.assertEqual(provider.metadata_listdir("subdir/"), ["file.txt"])
        self.assertEqual(provider.metadata_listdir("subdir\\"), ["file.txt"])
        self.assertEqual(provider.metadata_listdir("subdir/grandchild"), ["file2.txt"])
        self.assertEqual(provider.metadata_listdir("subdir\\grandchild"), ["file2.txt"])
        self.assertEqual(provider.metadata_listdir("subdir/grandchild/"), ["file2.txt"])
        self.assertEqual(
            provider.metadata_listdir("subdir\\grandchild\\"), ["file2.txt"]
        )

        with self.assertRaises(NotImplementedError):
            provider.run_script("foo", "ns")

        with self.assertRaises(NotImplementedError):
            provider.get_resource_filename(None, "foo")

        with self.assertRaises(NotImplementedError):
            provider.get_resource_stream(None, "foo")

        with self.assertRaises(NotImplementedError):
            provider.get_resource_string(None, "foo")

        with self.assertRaises(NotImplementedError):
            provider.has_resource("foo")

        with self.assertRaises(NotImplementedError):
            provider.resource_isdir("foo")

        with self.assertRaises(NotImplementedError):
            provider.resource_listdir("foo")


if __name__ == "__main__":
    unittest.main()
