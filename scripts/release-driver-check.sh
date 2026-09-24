#!/usr/bin/env bash
# Exercise the path a release user takes for an engine whose driver is not redistributed:
# install the jar the vendor provides, then run a statement with no development overrides, so the CLI
# resolves the JRE and the JDBC runner from the release manifest and loads the provided driver.
#
# usage: scripts/release-driver-check.sh <engine> <driver-jar> <data-dir> <connection-json> <statement>
set -euo pipefail
engine="${1:?engine required, for example db2}"
jar="${2:?path to the vendor driver jar required}"
data_dir="${3:?data directory required}"
connection="${4:?connection JSON required}"
statement="${5:?one statement required}"

sqlx_bin="${SQLX_BIN:-target/debug/sqlx}"
mkdir -p "${data_dir}"

# A declared component is not needed for these engines, so the check would fail if the CLI expected
# one; the jar has to be found through the driver directory alone.
"${sqlx_bin}" --data-dir "${data_dir}" driver add --type "${engine}" --jar "${jar}" >/dev/null

"${sqlx_bin}" --data-dir "${data_dir}" datasource add --name release-check --connection-stdin <<<"${connection}" >/dev/null

if ! result="$("${sqlx_bin}" --data-dir "${data_dir}" sql execute --datasource release-check --command "${statement}")"; then
  echo "the release path failed for ${engine}: ${result}" >&2
  exit 1
fi
case "${result}" in
  *'"success":true'*) ;;
  *)
    echo "the release path failed for ${engine}: ${result}" >&2
    exit 1
    ;;
esac
echo "release path: ${engine} installed a provided driver, resolved the runtime and answered a query"
