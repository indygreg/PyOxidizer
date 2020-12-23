# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

from contextlib import contextmanager, redirect_stderr
import importlib.util
from io import StringIO
import os
import pathlib
import sys
import tempfile
import unittest
from unittest.mock import patch
import warnings

from oxidized_importer import (
    OxidizedFinder,
    OxidizedResourceCollector,
    PythonModuleBytecode,
    find_resources_in_path,
)


@contextmanager
def assert_tempfile_cleaned_up(TemporaryDirectory=tempfile.TemporaryDirectory):
    call_count = 0
    class TrackingTempDir(TemporaryDirectory):
        def cleanup(self):
            nonlocal call_count
            call_count += 1
            super().cleanup()

    patcher = patch("tempfile.TemporaryDirectory", TrackingTempDir)
    msg = fr"""^Implicitly cleaning up <{TrackingTempDir.__name__} (['"]).*\1>$"""
    with warnings.catch_warnings(record=True) as cm, patcher:
        warnings.filterwarnings(
            "error", category=ResourceWarning, module=r"^tempfile$", message=msg)
        yield TrackingTempDir, TrackingTempDir.cleanup
    assert call_count == 1, f"tempfile.TemporaryDirectory.cleanup {call_count=}≠1"


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

        c = OxidizedResourceCollector(allowed_locations=["in-memory"])
        self.assertEqual(c.allowed_locations, ["in-memory"])

    def test_tempdir_error(self):
        class BadTempDir(tempfile.TemporaryDirectory):
            def cleanup(self):
                super().cleanup()
                raise FileNotFoundError(self.name)
        python_exe = os.environ.get("PYTHON_SYS_EXECUTABLE")
        c = OxidizedResourceCollector(allowed_locations=["in-memory"])
        stderr = StringIO()
        assertion = assert_tempfile_cleaned_up(BadTempDir)
        with assertion as (TrackingTempDir, cleanup), redirect_stderr(stderr):
            oxide = c.oxidize(python_exe=python_exe)
        self.assertRegex(
            stderr.getvalue(),
            fr"""Exception ignored in: <bound method {cleanup.__qualname__} """
            fr"""of <{TrackingTempDir.__name__} (['"]){tempfile.gettempdir()}"""
            fr"""[/\\].+\1>>""")

    def test_source_module(self):
        c = OxidizedResourceCollector(allowed_locations=["in-memory"])

        source_path = self.td / "foo.py"

        with source_path.open("wb") as fh:
            fh.write(b"import io\n")

        for resource in find_resources_in_path(self.td):
            c.add_in_memory(resource)

        f = OxidizedFinder()
        python_exe = os.environ.get("PYTHON_SYS_EXECUTABLE")
        with assert_tempfile_cleaned_up():
            oxide = c.oxidize(python_exe=python_exe)
        f.add_resources(oxide[0])

        resources = [r for r in f.indexed_resources() if r.name == "foo"]
        self.assertEqual(len(resources), 1)

        r = resources[0]
        self.assertEqual(r.in_memory_source, b"import io\n")

    def test_add_sys_path(self):
        c = OxidizedResourceCollector(
            allowed_locations=["in-memory", "filesystem-relative"]
        )

        for path in sys.path:
            if os.path.isdir(path):
                for resource in find_resources_in_path(path):
                    c.add_in_memory(resource)
                    c.add_filesystem_relative("", resource)

        python_exe = os.environ.get("PYTHON_SYS_EXECUTABLE")
        with assert_tempfile_cleaned_up():
            resources, file_installs = c.oxidize(python_exe=python_exe)
        f = OxidizedFinder()
        f.add_resources(resources)

        with (self.td / "serialized").open("wb") as fh:
            fh.write(f.serialize_indexed_resources())

        f = OxidizedFinder()
        f.index_file_memory_mapped(self.td / "serialized")

        self.assertGreaterEqual(len(f.indexed_resources()), len(resources))

        for r in f.indexed_resources():
            r.in_memory_source
            r.in_memory_bytecode

    def test_urllib(self):
        c = OxidizedResourceCollector(allowed_locations=["filesystem-relative"])

        for path in sys.path:
            if os.path.isdir(path):
                for resource in find_resources_in_path(path):
                    if isinstance(resource, PythonModuleBytecode):
                        if resource.module.startswith("urllib"):
                            if resource.optimize_level == 0:
                                c.add_filesystem_relative("lib", resource)

        python_exe = os.environ.get("PYTHON_SYS_EXECUTABLE")
        with assert_tempfile_cleaned_up():
            resources, file_installs = c.oxidize(python_exe=python_exe)
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
    unittest.main()
