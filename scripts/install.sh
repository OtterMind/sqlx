#!/usr/bin/env sh
set -eu

sqlx_base="${SQLX_RELEASE_BASE:-https://github.com/OtterMind/sqlx/releases}"
sqlx_bin="${SQLX_INSTALL_DIR:-${HOME}/.local/bin}"
sqlx_temp=$(mktemp -d)
trap 'rm -rf "$sqlx_temp"' EXIT HUP INT TERM
for sqlx_tool in curl unzip; do
  command -v "$sqlx_tool" >/dev/null 2>&1 || { echo "Required tool missing: $sqlx_tool" >&2; exit 1; }
done
case "$(uname -s)" in Darwin) sqlx_os=macos ;; Linux) sqlx_os=linux ;; *) echo 'Use install.ps1 for Windows.' >&2; exit 1 ;; esac
case "$(uname -m)" in arm64|aarch64) sqlx_arch=arm64 ;; x86_64|amd64) sqlx_arch=x64 ;; *) echo 'Unsupported CPU architecture.' >&2; exit 1 ;; esac
sqlx_version="${SQLX_VERSION:-}"
if [ -z "$sqlx_version" ]; then
  if ! curl --fail --silent --show-error --location "$sqlx_base/latest/download/release-version.txt" -o "$sqlx_temp/version"; then
    echo 'No downloadable release is available. See the README source-install instructions.' >&2
    exit 1
  fi
  sqlx_version=$(tr -d '\r\n' < "$sqlx_temp/version")
fi
case "$sqlx_version" in ''|*[!0-9A-Za-z.+-]*) echo 'Invalid release version.' >&2; exit 1 ;; esac
sqlx_asset="sqlx-$sqlx_os-$sqlx_arch.zip"
sqlx_url="$sqlx_base/download/v$sqlx_version"
curl --fail --silent --show-error --location "$sqlx_url/$sqlx_asset" -o "$sqlx_temp/$sqlx_asset"
curl --fail --silent --show-error --location "$sqlx_url/SHA256SUMS" -o "$sqlx_temp/SHA256SUMS"
sqlx_expected=$(awk -v asset="$sqlx_asset" '$2 == asset {print $1}' "$sqlx_temp/SHA256SUMS")
if command -v sha256sum >/dev/null 2>&1; then
  sqlx_actual=$(sha256sum "$sqlx_temp/$sqlx_asset" | awk '{print $1}')
else
  sqlx_actual=$(shasum -a 256 "$sqlx_temp/$sqlx_asset" | awk '{print $1}')
fi
if [ -z "$sqlx_expected" ] || [ "$sqlx_expected" != "$sqlx_actual" ]; then
  echo 'Download checksum verification failed.' >&2; exit 1
fi
mkdir -p "$sqlx_bin"
if [ -L "$sqlx_bin/sqlx" ]; then echo 'Existing sqlx is a symlink; choose another SQLX_INSTALL_DIR.' >&2; exit 1; fi
if [ -e "$sqlx_bin/sqlx" ]; then
  sqlx_existing=$("$sqlx_bin/sqlx" --version 2>/dev/null || true)
  case "$sqlx_existing" in *'(OtterMind/sqlx)'*) ;; *) echo 'Another sqlx executable exists; choose another SQLX_INSTALL_DIR.' >&2; exit 1 ;; esac
fi
unzip -p "$sqlx_temp/$sqlx_asset" sqlx > "$sqlx_temp/sqlx"
chmod 755 "$sqlx_temp/sqlx"
sqlx_new_version=$("$sqlx_temp/sqlx" --version)
case "$sqlx_new_version" in *'(OtterMind/sqlx)'*) ;; *) echo 'Archive is not an OtterMind SQLX executable.' >&2; exit 1 ;; esac
sqlx_stage=$(mktemp "$sqlx_bin/.sqlx-install.XXXXXX")
cp "$sqlx_temp/sqlx" "$sqlx_stage"
chmod 755 "$sqlx_stage"
mv -f "$sqlx_stage" "$sqlx_bin/sqlx"
echo "Installed $sqlx_new_version to $sqlx_bin/sqlx"
echo "Add $sqlx_bin to PATH, then run: sqlx init"
