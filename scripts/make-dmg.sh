#!/bin/bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Script to build DMG for PyOxidizer.

set -ex

# Clear out old state.
rm -rf build
rm -rf dmg_root
rm -f PyOxidizer.dmg

if [ -d /Volumes/PyOxidizer ]; then
	DEV_NAME=$(hdiutil info | egrep --color=never '^/dev/' | sed 1q | awk '{print $1}')
	hdiutil detach "${DEV_NAME}"
fi

if [ -n "${IN_CI}" ]; then
  PYOXIDIZER=dist/x86_64-apple-darwin/pyoxidizer
  chmod +x ${PYOXIDIZER}
else
  if [[ $(uname -m) == 'arm64' ]]; then
    PYOXIDIZER=target/aarch64-apple-darwin/release/pyoxidizer
  else
    PYOXIDIZER=target/x86_64-apple-darwin/release/pyoxidizer
  fi
fi

$PYOXIDIZER build --release --var-env IN_CI IN_CI macos_app_bundle

hdiutil create \
        -srcfolder build/*/release/macos_app_bundle \
        -volname PyOxidizer \
        -fs HFS+ \
        -fsargs "-c c=64,a=16,e=16" \
        -format UDRW \
        PyOxidizer

# Mount it.
DEV_NAME=$(hdiutil attach -readwrite -noverify -noautoopen PyOxidizer.dmg | egrep --color=never '^/dev/' | sed 1q | awk '{print $1}')

# Create a symlink to /Applications for drag and drop.
ln -s /Applications /Volumes/PyOxidizer/Applications

# Run AppleScript to create the .DS_Store.
/usr/bin/osascript scripts/dmg.applescript PyOxidizer

# Open this folder automatically when mounted.
bless --folder /Volumes/PyOxidizer --openfolder /Volumes/PyOxidizer

# Unmount.
hdiutil detach "${DEV_NAME}"

# Compress.
hdiutil convert PyOxidizer.dmg -format UDZO -imagekey zlib-level=9 -ov -o PyOxidizer.dmg
