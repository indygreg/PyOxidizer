#!/usr/bin/env bash
# Script to build macOS binary wheel for oxidized_importer.

set -exo pipefail

export CIBW_BUILD=cp38-macosx_x86_64

python3.8 -m cibuildwheel --output-dir wheelhouse --platform macos
