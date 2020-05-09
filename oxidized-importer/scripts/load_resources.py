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

    kwargs = {}

    if args.load_method == "memory":
        with open(args.resources_file, "rb") as fh:
            kwargs["resources_data"] = fh.read()
    elif args.load_method == "mmap":
        kwargs["resources_file"] = args.resources_file
    else:
        raise Exception("unhandled load method")

    for _ in range(args.iterations):
        oxidized_importer.OxidizedFinder(**kwargs)


if __name__ == "__main__":
    main()
