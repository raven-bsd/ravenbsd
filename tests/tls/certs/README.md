<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# T3 test certificate fixture

This directory generates the reusable test PKI for the T3 trust-store checks. The generated
files live under `build/tls/certs/test/` and are never committed; the production bundle staging
path `build/tls/certs/ca-bundle.pem` is separate.

| File | Purpose |
|---|---|
| `tls-certs.cnf` | OpenSSL CA config; `dir = build/tls/certs/pki`, 1970-2049 issue dates |
| `server.ext` | server extensions: `serverAuth`, SAN `IP:10.0.2.2`, key identifiers |
| `make-test-ca.sh` | generates the trusted CA/server and an unrelated untrusted CA/server |
| `verify.sh` | positive and negative chain, IP SAN and validity checks |

```sh
sh tests/tls/certs/make-test-ca.sh
sh tests/tls/certs/verify.sh
```

`verify.sh` expects these outcomes:

* trusted CA + trusted server: success;
* trusted CA + IP SAN `10.0.2.2`: success;
* trusted CA + IP SAN `10.0.2.3`: rejection;
* untrusted CA + trusted server: rejection;
* untrusted CA + its own server: success;
* trusted CA + untrusted server: rejection;
* CA validity covers 1970 through 2049;
* server SAN is exactly `IP Address:10.0.2.2`;
* the CA bundle has one certificate and no private key.

The T2 `tls-client` qtest is the target-side positive handshake and uses its own ephemeral
`/tls-ca.pem`; this fixture adds the reusable negative verification cases and the CA-bundle
shape checks for the trust store.
