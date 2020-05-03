# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import importlib.machinery
import os
import pathlib
import sys
import tempfile
import unittest

from oxidized_importer import (
    find_resources_in_path,
    PythonModuleBytecode,
    PythonModuleSource,
    PythonExtensionModule,
    PythonPackageDistributionResource,
    PythonPackageResource,
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

    def test_source_file(self):
        source_path = self.td / "foo.py"

        with source_path.open("wb") as fh:
            fh.write(b"import io\n")

        resources = find_resources_in_path(self.td)
        self.assertEqual(len(resources), 1)
        r = resources[0]
        self.assertIsInstance(r, PythonModuleSource)
        self.assertEqual(r.module, "foo")
        self.assertEqual(r.source, b"import io\n")
        self.assertFalse(r.is_package)

    def test_bytecode_file(self):
        cache_tag = sys.implementation.cache_tag

        path = self.td / "__pycache__" / ("foo.%s.pyc" % cache_tag)
        path.parent.mkdir()

        with path.open("wb") as fh:
            # First 16 bytes are a header, which gets stripped.
            fh.write(b"0123456789abcdefbytecode")

        resources = find_resources_in_path(self.td)
        self.assertEqual(len(resources), 1)
        r = resources[0]
        self.assertIsInstance(r, PythonModuleBytecode)
        self.assertEqual(r.module, "foo")
        self.assertEqual(r.bytecode, b"bytecode")
        self.assertEqual(r.optimize_level, 0)
        self.assertFalse(r.is_package)

    def test_extension_module(self):
        suffix = importlib.machinery.EXTENSION_SUFFIXES[0]

        path = self.td / ("foo%s" % suffix)

        with path.open("wb") as fh:
            fh.write(b"dummy")

        resources = find_resources_in_path(self.td)
        self.assertEqual(len(resources), 1)
        r = resources[0]
        self.assertIsInstance(r, PythonExtensionModule)
        self.assertEqual(r.name, "foo")

    def test_package_resource(self):
        init_py = self.td / "package" / "__init__.py"
        init_py.parent.mkdir()

        with init_py.open("wb"):
            pass

        resource = self.td / "package" / "resource.txt"
        with resource.open("wb") as fh:
            fh.write(b"resource file")

        resources = find_resources_in_path(self.td)
        self.assertEqual(len(resources), 2)

        r = resources[0]
        self.assertIsInstance(r, PythonModuleSource)
        self.assertEqual(r.module, "package")
        self.assertTrue(r.is_package)

        r = resources[1]
        self.assertIsInstance(r, PythonPackageResource)
        self.assertEqual(r.package, "package")
        self.assertEqual(r.name, "resource.txt")
        self.assertEqual(r.data, b"resource file")

    def test_package_distribution_resource(self):
        init_py = self.td / "foo" / "__init__.py"
        init_py.parent.mkdir()

        with init_py.open("wb"):
            pass

        resource = self.td / "foo-1.0.dist-info" / "METADATA"
        resource.parent.mkdir()

        with resource.open("wb") as fh:
            fh.write(b"Name: foo\n")
            fh.write(b"Version: 1.0\n")

        resources = find_resources_in_path(self.td)
        self.assertEqual(len(resources), 2)

        r = resources[0]
        self.assertIsInstance(r, PythonModuleSource)
        self.assertEqual(r.module, "foo")
        self.assertTrue(r.is_package)

        r = resources[1]
        self.assertIsInstance(r, PythonPackageDistributionResource)
        self.assertEqual(r.package, "foo")
        self.assertEqual(r.version, "1.0")
        self.assertEqual(r.name, "METADATA")
        self.assertEqual(r.data, b"Name: foo\nVersion: 1.0\n")

    def test_scan_missing(self):
        with self.assertRaisesRegex(ValueError, "path is not a directory"):
            find_resources_in_path(self.td / "missing")

    def test_scan_not_directory(self):
        path = self.td / "file"
        with path.open("wb"):
            pass

        with self.assertRaisesRegex(ValueError, "path is not a directory"):
            find_resources_in_path(path)

    def test_scan_sys_path(self):
        for path in sys.path:
            if os.path.isdir(path):
                find_resources_in_path(path)


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
