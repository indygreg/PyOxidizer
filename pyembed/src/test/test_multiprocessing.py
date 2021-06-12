# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import sys
import multiprocessing
import unittest


def multiply(x):
    return x * x


# We can't run on Windows because the "spawn" start method relies on
# spawning a new process. That process needs to recognize the
# --multiprocessing-fork argument. The choices of executables to spawn
# are the python.exe (which is the default in the test environment) and
# the Rust test executable. Neither of which recognize
# --multiprocessing-fork. So we're out of luck for testing "spawn" start
# methods.
@unittest.skipIf(sys.platform == "win32", "spawn method not supported")
class TestMultiprocessing(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        sys.frozen = True

        if sys.platform == "win32":
            multiprocessing.set_start_method("spawn", force=True)
        else:
            multiprocessing.set_start_method("fork", force=True)

    def test_pool(self):
        with multiprocessing.Pool(4) as p:
            p.map(multiply, range(1024))

        # Do it twice to ensure state isn't funky.
        with multiprocessing.Pool(4) as p:
            p.map(multiply, range(1024))


if __name__ == "__main__":
    unittest.main()
