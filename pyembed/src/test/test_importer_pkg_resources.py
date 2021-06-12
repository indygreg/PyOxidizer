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
    OxidizedPathEntryFinder,
    OxidizedPkgResourcesProvider,
    OxidizedResourceCollector,
    OxidizedResource,
    find_resources_in_path,
    register_pkg_resources,
)


def make_in_memory_finder():
    f = OxidizedFinder()

    r = OxidizedResource()
    r.is_module = True
    r.is_package = True
    r.name = "package0"
    r.in_memory_source = b"pass"
    r.in_memory_distribution_resources = {
        "METADATA": b"Name: package0\nVersion: 1.0\n",
        "entry_points.txt": b"[console_scripts]\ncli = package0:cli\n",
    }
    r.in_memory_package_resources = {
        "file0": b"foo",
    }
    f.add_resource(r)

    r = OxidizedResource()
    r.is_module = True
    r.is_package = True
    r.name = "package1"
    r.in_memory_source = b"pass"
    r.in_memory_distribution_resources = {
        "PKG-INFO": b"Name: package1\nVersion: 2.0\n",
        "requires.txt": b"package0\n",
    }
    f.add_resource(r)

    r = OxidizedResource()
    r.is_module = True
    r.is_package = True
    r.name = "package0.p0child0"
    r.in_memory_source = b"pass"
    r.in_memory_distribution_resources = {
        "METADATA": b"Name: p0child0\nVersion: 1.0\n",
    }
    r.in_memory_package_resources = {
        "childfile0": b"foo",
    }
    f.add_resource(r)

    r = OxidizedResource()
    r.is_module = True
    r.is_package = True
    r.name = "package0.p0child0.p0grandchild0"
    r.in_memory_source = b"pass"
    r.in_memory_distribution_resources = {
        "METADATA": b"Name: p0grandchild0\nVersion: 1.0\n",
    }
    f.add_resource(r)

    return f


def install_distributions_finder():
    f = make_in_memory_finder()
    sys.meta_path.insert(0, f)
    sys.path_hooks.insert(0, f.path_hook)

    return f


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
        self.old_path_hooks = list(sys.path_hooks)
        self.old_path_importer_cache = dict(sys.path_importer_cache)
        self.old_modules = dict(sys.modules)
        self.old_provider_factories = dict(pkg_resources._provider_factories)
        self.old_distribution_finders = dict(pkg_resources._distribution_finders)

    def tearDown(self):
        self.raw_temp_dir.cleanup()
        del self.raw_temp_dir
        del self.td
        sys.meta_path[:] = self.old_finders
        sys.path[:] = self.old_path
        sys.path_hooks[:] = self.old_path_hooks
        sys.path_importer_cache.clear()
        sys.path_importer_cache.update(self.old_path_importer_cache)
        sys.modules.clear()
        sys.modules.update(self.old_modules)
        pkg_resources._provider_factories.clear()
        pkg_resources._provider_factories.update(self.old_provider_factories)
        pkg_resources._distribution_finders.clear()
        pkg_resources._distribution_finders.update(self.old_distribution_finders)

    def _write_metadata(self):
        metadata_path = self.td / "my_package-1.0.dist-info" / "METADATA"
        metadata_path.parent.mkdir()

        with metadata_path.open("wb") as fh:
            fh.write(b"Name: my_package\n")
            fh.write(b"Version: 1.0\n")

    def _finder_from_td(self):
        collector = OxidizedResourceCollector(allowed_locations=["in-memory"])
        for r in find_resources_in_path(self.td):
            collector.add_in_memory(r)

        f = OxidizedFinder()
        f.add_resources(collector.oxidize()[0])

        return f

    def test_provider_registered(self):
        # Should have been done via setUpClass.
        self.assertEqual(
            pkg_resources._distribution_finders.get(OxidizedPathEntryFinder).__name__,
            "pkg_resources_find_distributions",
        )
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

    def assert_package0(self, dist):
        self.assertIsInstance(dist, pkg_resources.Distribution)
        self.assertEqual(dist.project_name, "package0")
        self.assertEqual(dist.version, "1.0")
        self.assertEqual(list(dist.get_entry_map(None).keys()), ["console_scripts"])

    def assert_package0_child0(self, dist):
        self.assertIsInstance(dist, pkg_resources.Distribution)
        self.assertEqual(dist.project_name, "p0child0")
        self.assertEqual(dist.version, "1.0")

    def assert_package0_grandchild0(self, dist):
        self.assertIsInstance(dist, pkg_resources.Distribution)
        self.assertEqual(dist.project_name, "p0grandchild0")
        self.assertEqual(dist.version, "1.0")

    def assert_package1(self, dist):
        self.assertIsInstance(dist, pkg_resources.Distribution)
        self.assertEqual(dist.project_name, "package1")
        self.assertEqual(dist.version, "2.0")
        self.assertEqual(dist.requires(), [pkg_resources.Requirement("package0")])

    def test_finder_attribute(self):
        f = install_distributions_finder()
        self.assertTrue(f.pkg_resources_import_auto_register)

    def test_find_distributions_no_path_hooks(self):
        f = install_distributions_finder()
        sys.path_hooks.clear()
        self.assertEqual(
            list(pkg_resources.find_distributions(f.path_hook_base_str, only=True)), []
        )

    def test_find_distributions_not_path_hook_base_str_path_hook(self):
        install_distributions_finder()
        self.assertEqual(list(pkg_resources.find_distributions("/some/other/path")), [])

    def test_find_distributions_top_level_only_false(self):
        f = install_distributions_finder()

        dists = list(pkg_resources.find_distributions(f.path_hook_base_str, only=False))
        self.assertEqual(len(dists), 4)
        self.assert_package0(dists[0])
        self.assert_package0_child0(dists[1])
        self.assert_package0_grandchild0(dists[2])
        self.assert_package1(dists[3])

    def test_find_distributions_top_level_only_true(self):
        f = install_distributions_finder()
        search_path = f.path_hook_base_str

        dists = list(pkg_resources.find_distributions(search_path, only=True))
        self.assertEqual(len(dists), 2)
        self.assert_package0(dists[0])
        self.assert_package1(dists[1])

    def test_find_distributions_search_path_missing(self):
        f = install_distributions_finder()
        search_path = os.path.join(f.path_hook_base_str, "missing_package")

        dists = list(pkg_resources.find_distributions(search_path, only=False))
        self.assertEqual(dists, [])

        dists = list(pkg_resources.find_distributions(search_path, only=True))
        self.assertEqual(dists, [])

    def test_find_distributions_search_path_child_only_false(self):
        f = install_distributions_finder()
        search_path = os.path.join(f.path_hook_base_str, "package0")

        dists = list(pkg_resources.find_distributions(search_path, only=False))
        self.assertEqual(len(dists), 2)
        self.assert_package0_child0(dists[0])
        self.assert_package0_grandchild0(dists[1])

    def test_find_distributions_search_path_child_only_true(self):
        f = install_distributions_finder()
        search_path = os.path.join(f.path_hook_base_str, "package0")

        dists = list(pkg_resources.find_distributions(search_path, only=True))
        self.assertEqual(len(dists), 1)
        self.assert_package0_child0(dists[0])

    def test_find_distributions_search_path_grandchild_only_false(self):
        f = install_distributions_finder()
        search_path = os.path.join(f.path_hook_base_str, "package0", "p0child0")

        dists = list(pkg_resources.find_distributions(search_path, only=False))
        self.assertEqual(len(dists), 1)
        self.assert_package0_grandchild0(dists[0])

    def test_find_distributions_search_path_grandchild_only_true(self):
        f = install_distributions_finder()
        search_path = os.path.join(f.path_hook_base_str, "package0", "p0child0")

        dists = list(pkg_resources.find_distributions(search_path, only=True))
        self.assertEqual(len(dists), 1)
        self.assert_package0_grandchild0(dists[0])

    def test_find_distributions_search_path_too_deep(self):
        f = install_distributions_finder()
        search_path = os.path.join(
            f.path_hook_base_str, "package0", "p0child0", "grandchild0"
        )

        self.assertEqual(
            list(pkg_resources.find_distributions(search_path, only=False)), []
        )
        self.assertEqual(
            list(pkg_resources.find_distributions(search_path, only=True)), []
        )

    def test_resource_manager(self):
        install_distributions_finder()

        self.assertTrue(pkg_resources.resource_exists("package0", "file0"))
        self.assertTrue(
            pkg_resources.resource_exists("package0.p0child0", "childfile0")
        )
        self.assertFalse(pkg_resources.resource_exists("package0", "missing"))
        self.assertFalse(pkg_resources.resource_exists("package0.p0child0", "missing"))

        with self.assertRaises(ModuleNotFoundError):
            pkg_resources.resource_exists("missing_package", "irrelevant")

        self.assertIsInstance(
            pkg_resources.resource_stream("package0", "file0"), io.BytesIO
        )
        self.assertIsInstance(
            pkg_resources.resource_stream("package0.p0child0", "childfile0"), io.BytesIO
        )
        with self.assertRaises(OSError):
            pkg_resources.resource_stream("package0", "missing")
        with self.assertRaises(OSError):
            pkg_resources.resource_stream("package0.p0child0", "missing")

        with self.assertRaises(NotImplementedError):
            pkg_resources.resource_filename("package0", "file0")


if __name__ == "__main__":
    unittest.main()
