# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import sys
import unittest

from _pyoxidizer_importer import PyOxidizerFinder as Finder


class TestImporterConstruction(unittest.TestCase):
    def test_no_args(self):
        f = Finder()
        self.assertIsInstance(f, Finder)

    def test_none(self):
        f = Finder(None)
        self.assertIsInstance(f, Finder)

        f = Finder(resources=None)
        self.assertIsInstance(f, Finder)

    def test_bad_resources_type(self):
        with self.assertRaises(TypeError):
            Finder("foo")

    def test_resources_no_magic(self):
        with self.assertRaisesRegex(ValueError, "reading 8 byte"):
            Finder(b"foo")

    def test_resources_bad_magic(self):
        with self.assertRaisesRegex(ValueError, "unrecognized file format"):
            Finder(b"\xde\xad\xbe\xef\xaa\xaa\xaa\xaa")

    def test_no_indices(self):
        Finder(b"pyembed\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00")


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
