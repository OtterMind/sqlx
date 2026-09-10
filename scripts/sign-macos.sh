#!/usr/bin/env bash
set -euo pipefail
# The Java runtime is downloaded from the vendor; do not re-sign vendor JVM libraries.
for binary in sqlx sqlx-driver-mysql sqlx-driver-postgres; do
  codesign --force --options runtime --timestamp \
    --keychain "${CHAT2DB_SIGNING_KEYCHAIN:?}" \
    --sign "${APPLE_SIGNING_IDENTITY:?}" "target/release/${binary}"
  codesign --verify --strict --verbose=2 "target/release/${binary}"
done
