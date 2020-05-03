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
        self.assertIsInstance(resources[0], PythonModuleSource)

    def test_bytecode_file(self):
        cache_tag = sys.implementation.cache_tag

        path = self.td / "__pycache__" / ("foo.%s.pyc" % cache_tag)
        path.parent.mkdir()

        with path.open("wb") as fh:
            fh.write(b"dummy")

        resources = find_resources_in_path(self.td)
        self.assertEqual(len(resources), 1)
        self.assertIsInstance(resources[0], PythonModuleBytecode)

    def test_extension_module(self):
        suffix = importlib.machinery.EXTENSION_SUFFIXES[0]

        path = self.td / ("foo%s" % suffix)

        with path.open("wb") as fh:
            fh.write(b"dummy")

        resources = find_resources_in_path(self.td)
        self.assertEqual(len(resources), 1)
        self.assertIsInstance(resources[0], PythonExtensionModule)

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
        self.assertIsInstance(resources[0], PythonModuleSource)
        self.assertIsInstance(resources[1], PythonPackageResource)

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
        self.assertIsInstance(resources[0], PythonModuleSource)
        self.assertIsInstance(resources[1], PythonPackageDistributionResource)

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
