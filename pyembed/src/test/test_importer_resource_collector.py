# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import importlib.util
import os
import pathlib
import sys
import tempfile
import unittest

from oxidized_importer import (
    OxidizedFinder,
    OxidizedResourceCollector,
    PythonModuleBytecode,
    find_resources_in_path,
)


class TestImporterResourceCollector(unittest.TestCase):
    def setUp(self):
        self.raw_temp_dir = tempfile.TemporaryDirectory(
            prefix="oxidized_importer-test-"
        )
        self.td = pathlib.Path(self.raw_temp_dir.name)

    def tearDown(self):
        self.raw_temp_dir.cleanup()
        del self.raw_temp_dir
        del self.td

    def test_construct(self):
        with self.assertRaises(TypeError):
            OxidizedResourceCollector()

        c = OxidizedResourceCollector(policy="in-memory-only")
        self.assertEqual(c.policy, "in-memory-only")

    def test_source_module(self):
        c = OxidizedResourceCollector(policy="in-memory-only")

        source_path = self.td / "foo.py"

        with source_path.open("wb") as fh:
            fh.write(b"import io\n")

        for resource in find_resources_in_path(self.td):
            c.add_in_memory(resource)

        f = OxidizedFinder()
        f.add_resources(c.oxidize()[0])

        resources = [r for r in f.indexed_resources() if r.name == "foo"]
        self.assertEqual(len(resources), 1)

        r = resources[0]
        self.assertEqual(r.in_memory_source, b"import io\n")

    def test_add_sys_path(self):
        c = OxidizedResourceCollector(
            policy="prefer-in-memory-fallback-filesystem-relative:prefix"
        )

        for path in sys.path:
            if os.path.isdir(path):
                for resource in find_resources_in_path(path):
                    c.add_in_memory(resource)
                    c.add_filesystem_relative("", resource)

        resources, file_installs = c.oxidize()
        f = OxidizedFinder()
        f.add_resources(resources)

        with (self.td / "serialized").open("wb") as fh:
            fh.write(f.serialize_indexed_resources())

        f = OxidizedFinder(resources_file=self.td / "serialized")

        self.assertGreaterEqual(len(f.indexed_resources()), len(resources))

        for r in f.indexed_resources():
            r.in_memory_source
            r.in_memory_bytecode

    def test_urllib(self):
        c = OxidizedResourceCollector(policy="filesystem-relative-only:lib")

        for path in sys.path:
            if os.path.isdir(path):
                for resource in find_resources_in_path(path):
                    if isinstance(resource, PythonModuleBytecode):
                        if resource.module.startswith("urllib"):
                            if resource.optimize_level == 0:
                                c.add_filesystem_relative("lib", resource)

        resources, file_installs = c.oxidize()
        self.assertEqual(len(resources), len(file_installs))

        idx = None
        for i, resource in enumerate(resources):
            if resource.name == "urllib.request":
                idx = i
                break

        self.assertIsNotNone(idx)

        (path, data, executable) = file_installs[idx]
        self.assertEqual(
            path,
            pathlib.Path("lib")
            / "urllib"
            / "__pycache__"
            / ("request.%s.pyc" % sys.implementation.cache_tag),
        )

        self.assertTrue(data.startswith(importlib.util.MAGIC_NUMBER))


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
