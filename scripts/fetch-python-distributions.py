#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Emit python_distributions.rs boilerplate for python-build-standalone release."""

import argparse
import hashlib
import urllib.request

from github import Github


ENTRY = """
PythonDistributionRecord {{
    python_major_minor_version: "{major_minor}".to_string(),
    location: PythonDistributionLocation::Url {{
        url: "{url}".to_string(),
        sha256: "{sha256}".to_string(),
    }},
    target_triple: "{target_triple}".to_string(),
    supports_prebuilt_extension_modules: {supports_prebuilt_extension_modules},
}},
""".strip()


def download_and_hash(url):
    with urllib.request.urlopen(url) as r:
        h = hashlib.sha256()

        while True:
            chunk = r.read(32768)
            if not chunk:
                break

            h.update(chunk)

        return h.hexdigest()



def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--api-token", help="GitHub API token", required=True)
    parser.add_argument("--tag", help="python-build-standalone release tag", required=True)

    args = parser.parse_args()

    g = Github(args.api_token)

    repo = g.get_repo("indygreg/python-build-standalone")

    release = repo.get_release(args.tag)

    records = {}

    for asset in release.get_assets():
        name = asset.name
        url = asset.browser_download_url
        sha256 = download_and_hash(url)

        # cpython-3.8.6-i686-pc-windows-msvc-shared-pgo-20201021T0032.tar.zst

        assert name.startswith("cpython-")
        assert name.endswith(".tar.zst")

        parts = name.split("-")

        parts = parts[:-1]

        _python_flavor = parts.pop(0)
        major_minor = parts.pop(0)[0:3]

        if parts[-2] in ("shared", "static"):
            target_triple = "-".join(parts[0:-2])
            flavor = "-".join(parts[-2:])
        else:
            target_triple = "-".join(parts[0:-1])
            flavor = parts[-1]

        supports_prebuilt_extension_modules = target_triple != "x86_64-unknown-linux-musl" and \
            flavor != "static-noopt"

        key = "%s-%s-%s" % (major_minor, target_triple, flavor)

        records[key] = {
            "name": name,
            "url": url,
            "sha256": sha256,
            "major_minor": major_minor,
            "target_triple": target_triple,
            "supports_prebuilt_extension_modules": "true" if supports_prebuilt_extension_modules else "false",
        }

    lines = [
        "// Linux glibc linked.",
        ENTRY.format(**records["3.8-x86_64-unknown-linux-gnu-pgo"]),
        ENTRY.format(**records["3.9-x86_64-unknown-linux-gnu-pgo"]),
        "",
        "// Linux musl.",
        ENTRY.format(**records["3.8-x86_64-unknown-linux-musl-noopt"]),
        ENTRY.format(**records["3.9-x86_64-unknown-linux-musl-noopt"]),
        "",
        "// The order here is important because we will choose the",
        "// first one. We prefer shared distributions on Windows because",
        "// they are more versatile: statically linked Windows distributions",
        "// don't declspec(dllexport) Python symbols and can't load shared",
        "// shared library Python extensions, making them a pain to work",
        "// with.",
        "",
        "// Windows shared.",
        ENTRY.format(**records["3.8-i686-pc-windows-msvc-shared-pgo"]),
        ENTRY.format(**records["3.9-i686-pc-windows-msvc-shared-pgo"]),
        ENTRY.format(**records["3.8-x86_64-pc-windows-msvc-shared-pgo"]),
        ENTRY.format(**records["3.9-x86_64-pc-windows-msvc-shared-pgo"]),
        "",
        "// Windows static.",
        ENTRY.format(**records["3.8-i686-pc-windows-msvc-static-noopt"]),
        ENTRY.format(**records["3.9-i686-pc-windows-msvc-static-noopt"]),
        ENTRY.format(**records["3.8-x86_64-pc-windows-msvc-static-noopt"]),
        ENTRY.format(**records["3.9-x86_64-pc-windows-msvc-static-noopt"]),
        "",
        "// macOS.",
        ENTRY.format(**records["3.9-aarch64-apple-darwin-pgo"]),
        ENTRY.format(**records["3.8-x86_64-apple-darwin-pgo"]),
        ENTRY.format(**records["3.9-x86_64-apple-darwin-pgo"]),
    ]

    print("\n".join(lines))


if __name__ == "__main__":
    main()
