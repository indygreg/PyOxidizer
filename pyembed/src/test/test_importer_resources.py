# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import sys
import unittest

from _pyoxidizer_importer import (
    OxidizedResource,
    PyOxidizerFinder as Finder,
)


class TestImporterConstruction(unittest.TestCase):
    def test_resources_builtins(self):
        f = Finder()
        resources = f.indexed_resources()
        self.assertIsInstance(resources, list)
        self.assertGreater(len(resources), 0)
        self.assertIsInstance(resources[0], OxidizedResource)


if __name__ == "__main__":
    # Reset command arguments so test runner isn't confused.
    sys.argv[1:] = []
    unittest.main(exit=False)
