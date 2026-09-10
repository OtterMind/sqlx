#!/usr/bin/env bash
set -euo pipefail
for archive in dist/*.zip; do
  report="${RUNNER_TEMP:?}/$(basename "${archive}").notary.json"
  xcrun notarytool submit "${archive}" --wait --output-format json \
    --keychain "${CHAT2DB_SIGNING_KEYCHAIN:?}" \
    --keychain-profile "${CHAT2DB_NOTARY_KEYCHAIN_PROFILE:?}" > "${report}"
  python3 - "${report}" <<'PY'
import json,sys
report=json.load(open(sys.argv[1]))
if report.get('status')!='Accepted':raise SystemExit('Apple notarization was not accepted')
print('Notarization accepted:',report['id'])
PY
done
