# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
        f.index_bytes(b"pyembed\x03\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00")

    def test_origin_bad_value(self):
        with self.assertRaises(TypeError):
            OxidizedFinder(relative_path_origin=True)

    def test_origin(self):
        OxidizedFinder(relative_path_origin="/path/to/origin")


if __name__ == "__main__":
    unittest.main()
