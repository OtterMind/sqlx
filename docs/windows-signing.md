# Windows code signing

`packages.yml` builds Windows executables on a GitHub-hosted Windows runner,
passes their artifact ID to `sign-windows-artifacts.yml`, and downloads the
signed artifact by ID on a fresh Windows runner. The Windows job checks every
signature, the signing certificate's SHA-256 fingerprint, and the timestamp
before packaging. Manifest entries and archive hashes describe the signed files.
The release assembly depends on successful Windows verification and packaging.

The signed files are `sqlx.exe`, `sqlx-driver-mysql.exe`,
`sqlx-driver-postgres.exe`, `sqlx-driver-redis.exe`, `sqlx-driver-mongodb.exe`,
`sqlx-driver-sqlite.exe`, `sqlx-driver-duckdb.exe`, and `sqlx-ui.exe`.
Vendor JRE executables and libraries retain their original signatures.

## Signing host

Register a macOS ARM64 self-hosted runner in this repository with the custom
label `sqlx-windows-signing`. Use a separate runner installation and work
directory from other repositories. Run its service as the account that already
has access to the hardware certificate, Java, and the local signing script.
Install the runner's launchd service with its generated `svc.sh`.

Set these repository Actions variables:

| Variable | Value |
| --- | --- |
| `WINDOWS_SIGN_SCRIPT` | Absolute path to the host's shared signing entry point |
| `WINDOWS_SIGN_CERT_SHA256` | 64 hexadecimal characters identifying the expected leaf certificate's SHA-256 fingerprint |

The script accepts `--alg SHA-256 <file>`, produces an Authenticode signature
with a trusted timestamp, and returns a nonzero exit code on failure. Use an
absolute path in the variable; shell variables and `~` are not expanded there.
Keep the signing script, PKCS#11 configuration, certificate chain, and PIN on
the host, outside protected Desktop/Documents directories. The private key
stays in the hardware token. No signing credentials are passed through GitHub.

Every runner using the same token must call the shared locking entry point.
The host entry point uses macOS `lockf -k -t 600` around the existing signer,
with a single persistent lock file beside the script. It serializes hardware
access across repositories and returns the signer's exit status. Keep the lock
file in place; its existence alone does not indicate an active lock.

The runner needs outbound access to GitHub and Actions artifact storage. Use
the host's working network; do not copy obsolete proxy variables from another
runner. Remove stale `http_proxy`, `https_proxy`, `HTTP_PROXY`, and `HTTPS_PROXY`
entries from its `.env` and restart the runner when changing this configuration.
The workflow does not need inbound SSH access to the host.

## Trusted workflow sources

Signing is enabled only in `OtterMind/sqlx` for `v*` tag pushes and manual
dispatches from `main`. The Windows build also requires the source commit to
be an ancestor of `origin/main`. Signing jobs do not check out source code or
execute downloaded executables. They receive exactly the eight expected files
in a new temporary directory and remove the directory when finished.

For this public repository, require approval for **all external contributors'**
workflow runs. Do not approve untrusted workflows that request the signing
runner, and restrict writes to main and release tags to trusted maintainers.
Runner labels route jobs; they are not an authorization boundary. Do not add
pull request triggers to the signing workflow.

## Release behavior

Running Packages manually from main with `publish=false` creates the signed
release bundle without publishing it. A signed release must use a new version:
signing changes executable and archive hashes, so do not replace published
archives or manifests in place. Offline bundles must be assembled from that
version's signed release assets and retain the documented user-data backup and
merge behavior.

Customers whose WDAC policy restricts allowed publishers need their IT team to
allow this signing publisher. A valid signature is checked against the local
application-control policy when Windows starts the executable.
