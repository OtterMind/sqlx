#!/usr/bin/env bash
set -euo pipefail
# Sign every executable that scripts/package.py puts into a release archive: notarization rejects
# an archive as soon as one of its executables is unsigned. The Java runtime is downloaded from the
# vendor, so do not re-sign vendor JVM libraries.
binaries=$(python3 -c 'import sys;sys.path.insert(0,"scripts");from package import BINARIES;print(" ".join(BINARIES.values()))')
for binary in ${binaries}; do
  codesign --force --options runtime --timestamp \
    --keychain "${CHAT2DB_SIGNING_KEYCHAIN:?}" \
    --sign "${APPLE_SIGNING_IDENTITY:?}" "target/release/${binary}"
  codesign --verify --strict --verbose=2 "target/release/${binary}"
done
