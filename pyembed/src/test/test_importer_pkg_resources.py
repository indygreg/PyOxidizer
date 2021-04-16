# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# email.parser may be unused. However, it is needed by Rust code and some
# sys.path mucking in tests may prevent it from being imported. So import
# here to ensure it is cached in sys.modules so Rust can import it.
import email.parser

import io
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

        with (my_package_path / "resource0.txt").open("wb") as fh:
            fh.write(b"line0\n")
            fh.write(b"line1\n")

        (my_package_path / "subdir").mkdir()
        (my_package_path / "subdir" / "grandchild").mkdir()

        with (my_package_path / "subdir" / "child0.txt").open("wb"):
            pass

        with (my_package_path / "subdir" / "grandchild" / "grandchild.txt").open("wb"):
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

        with self.assertRaises(IOError):
            provider.get_resource_stream(None, "missing")

        fh = provider.get_resource_stream(None, "resource0.txt")
        self.assertIsInstance(fh, io.BytesIO)
        self.assertEqual(fh.read(), b"line0\nline1\n")

        with self.assertRaises(IOError):
            provider.get_resource_string(None, "missing")

        self.assertEqual(
            provider.get_resource_string(None, "resource0.txt"), b"line0\nline1\n"
        )
        self.assertEqual(provider.get_resource_string(None, "subdir/child0.txt"), b"")

        self.assertFalse(provider.has_resource("missing"))
        self.assertTrue(provider.has_resource("resource0.txt"))
        self.assertTrue(provider.has_resource("subdir/child0.txt"))
        self.assertTrue(provider.has_resource("subdir/grandchild/grandchild.txt"))

        self.assertFalse(provider.resource_isdir("missing"))
        self.assertFalse(provider.resource_isdir("resource0.txt"))
        self.assertFalse(provider.resource_isdir(""))
        self.assertTrue(provider.resource_isdir("subdir"))
        self.assertTrue(provider.resource_isdir("subdir/"))
        self.assertTrue(provider.resource_isdir("subdir\\"))
        self.assertTrue(provider.resource_isdir("subdir/grandchild"))
        self.assertTrue(provider.resource_isdir("subdir/grandchild/"))
        self.assertTrue(provider.resource_isdir("subdir\\grandchild"))
        self.assertTrue(provider.resource_isdir("subdir\\grandchild\\"))

        self.assertEqual(provider.resource_listdir("missing"), [])
        self.assertEqual(provider.resource_listdir(""), ["resource0.txt"])
        self.assertEqual(provider.resource_listdir("subdir"), ["child0.txt"])
        self.assertEqual(provider.resource_listdir("subdir/"), ["child0.txt"])
        self.assertEqual(provider.resource_listdir("subdir\\"), ["child0.txt"])
        self.assertEqual(
            provider.resource_listdir("subdir/grandchild"), ["grandchild.txt"]
        )
        self.assertEqual(
            provider.resource_listdir("subdir\\grandchild"), ["grandchild.txt"]
        )
        self.assertEqual(
            provider.resource_listdir("subdir/grandchild/"), ["grandchild.txt"]
        )
        self.assertEqual(
            provider.resource_listdir("subdir\\grandchild\\"), ["grandchild.txt"]
        )


if __name__ == "__main__":
    unittest.main()
