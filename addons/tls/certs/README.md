<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TLS CA bundle

This directory owns the CA trust-store staging and install helper for the OpenSSL add-on.
The full policy, verification defaults and update procedure are in
[`docs/tls-trust.md`](../../../docs/tls-trust.md).

| | |
|---|---|
| Staged bundle | `build/tls/certs/ca-bundle.pem` (generated, never committed) |
| Installed bundle | `/addons/etc/ssl/certs/ca-bundle.pem` on the target image |
| Source | host `ca-certificates` PEM bundle, or `--source <path>` |
| Helper | `sh addons/tls/certs/update-bundle.sh` |

The bundle is a PEM file with one or more `BEGIN CERTIFICATE` blocks and no private keys.
`update-bundle.sh --check` validates the host bundle and prints its certificate count, SHA-256
and first subject; `--stage` copies it under `build/`; `--install <root>` writes it under the
add-on prefix of an image root. No network access is used.

The T3 test PKI under `tests/tls/certs/` is separate: it is generated on the fly and is not a
production trust store. See that directory's README.
