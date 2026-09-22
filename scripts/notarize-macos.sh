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
if report.get('status')!='Accepted':
    # The report names the rejected archive and its issues, which is the only place Apple explains
    # why a submission failed.
    print(json.dumps(report,indent=2),file=sys.stderr)
    raise SystemExit('Apple notarization was not accepted')
print('Notarization accepted:',report['id'])
PY
done
