#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Benchmarking script for resources loading."""

import argparse
import oxidized_importer


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--iterations", type=int, default=1000)
    parser.add_argument("--load-method", choices={"memory", "mmap"}, default="memory")
    parser.add_argument("resources_file")

    args = parser.parse_args()

    data = None
    path = None

    if args.load_method == "memory":
        with open(args.resources_file, "rb") as fh:
            data = fh.read()
    elif args.load_method == "mmap":
        path = args.resources_file
    else:
        raise Exception("unhandled load method")

    for _ in range(args.iterations):
        oxidized_importer.OxidizedFinder()

        if data is not None:
            oxidized_importer.index_bytes(data)
        if path is not None:
            oxidized_importer.index_file_memory_mapped(path)


if __name__ == "__main__":
    main()
