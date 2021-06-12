# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import sys
import unittest

from oxidized_importer import OxidizedFinder


class TestImporterConstruction(unittest.TestCase):
    def test_no_args(self):
        f = OxidizedFinder()
        self.assertIsInstance(f, OxidizedFinder)

    def test_none(self):
        f = OxidizedFinder(None)
        self.assertIsInstance(f, OxidizedFinder)

    def test_bad_resources_type(self):
        with self.assertRaises(TypeError):
            f = OxidizedFinder()
            f.index_bytes("foo")

    def test_resources_no_magic(self):
        with self.assertRaisesRegex(ValueError, "reading 8 byte"):
            f = OxidizedFinder()
            f.index_bytes(b"foo")

    def test_resources_bad_magic(self):
        with self.assertRaisesRegex(ValueError, "unrecognized file format"):
            f = OxidizedFinder()
            f.index_bytes(b"\xde\xad\xbe\xef\xaa\xaa\xaa\xaa")

    def test_no_indices(self):
        f = OxidizedFinder()
        f.index_bytes(
            b"pyembed\x03\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
        )

    def test_multiprocessing_set_start_method(self):
        f = OxidizedFinder()
        self.assertIsNone(f.multiprocessing_set_start_method)

    def test_origin_bad_value(self):
        with self.assertRaises(TypeError):
            OxidizedFinder(relative_path_origin=True)

    def test_path_hook_base_str(self):
        f = OxidizedFinder()
        # We can't make reasonable assumptions about the value of path_hook_base_str
        # because the test environment does weird things with sys.argv and
        # hence sys.executable. We have to rely on other tests for the
        # correctness of this value.
        self.assertIsInstance(f.path_hook_base_str, str)

    def test_origin(self):
        f = OxidizedFinder(relative_path_origin="/path/to/origin")
        self.assertEqual(f.origin, "/path/to/origin")


if __name__ == "__main__":
    unittest.main()
