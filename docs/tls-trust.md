<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TLS trust store and certificate verification

Status: T3 implementation. The trust store is a PEM CA bundle installed with the OpenSSL add-on;
`crates/raven-tls` always verifies the chain and the peer name and has no unverified mode. The
target-side proof is the T2 `tls-client` qtest; the T3 fixture adds reusable positive and
negative verification checks.

Evidence tags: **OBSERVED** (read or run here), **PROPOSED** (design, not yet wired into a product
image), **UNVERIFIED** (not checked on hardware).

## Trust-store layout

| | |
|---|---|
| Staged bundle | `build/tls/certs/ca-bundle.pem` — generated, never committed |
| Installed bundle | `/addons/etc/ssl/certs/ca-bundle.pem` on the target image |
| Source | the host `ca-certificates` PEM bundle, or `--source <path>` |
| Format | PEM, one or more `BEGIN CERTIFICATE` blocks, no private keys |
| Helper | `sh addons/tls/certs/update-bundle.sh` |

The add-on prefix is `/addons` (see `docs/tls.md`). The bundle is installed below that prefix,
not under `/etc`: the `server` profile mounts a writable ramfs on `/etc`, so a trust store there
would disappear on the next boot. The path is a packaging convention; the wrapper itself takes an
explicit CA file and never reads an implicit environment variable.

```sh
sh addons/tls/certs/update-bundle.sh --check
sh addons/tls/certs/update-bundle.sh --stage
sh addons/tls/certs/update-bundle.sh --install <image-root>
```

`--check` prints the source path, certificate count, SHA-256 and first subject. `--stage` copies
the bundle to `build/tls/certs/ca-bundle.pem`; `--install <root>` writes
`<root>/addons/etc/ssl/certs/ca-bundle.pem`. The script rejects an empty bundle, a bundle with no
PEM certificate and a bundle containing a private key. It uses the host openssl and never the
network.

## Verification defaults

The policy lives in `crates/raven-tls/src/lib.rs` and is on unconditionally:

* `SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, NULL)` loads the peer chain; there is no
  `SSL_VERIFY_NONE` path and no API option to disable it. **OBSERVED**
* `SSL_CTX_load_verify_locations(ctx, ca_file, NULL)` trusts exactly the supplied bundle. A
  missing or malformed bundle returns `Error::CaBundle`; the handshake is not attempted. **OBSERVED**
* `SSL_set1_host` (with SNI) is used for DNS names and `X509_VERIFY_PARAM_set1_ip_asc` for IPv4
  literals. The expected name cannot be omitted. **OBSERVED**
* After the handshake, `SSL_get_verify_result` must be `X509_V_OK`; any other code becomes
  `Error::Verify(code)`. **OBSERVED**
* `connect` is the only entry point. It does not retry with a different CA and does not fall
  back to an unverified connection. **OBSERVED**

The T2 qtest exercises the positive path from the target: the client trusts `/tls-ca.pem`, checks
the `IP:10.0.2.2` SAN and exits 0 only after the handshake and byte exchange. The host name in
the test is an IPv4 literal, so the test selects the `X509_VERIFY_PARAM_set1_ip_asc` branch.

## Test CA fixture

`tests/tls/certs/` generates the reusable fixture under `build/tls/certs/test/`:

```sh
sh tests/tls/certs/make-test-ca.sh
sh tests/tls/certs/verify.sh
```

`make-test-ca.sh` creates a trusted CA and a server certificate for `IP:10.0.2.2`, both issued
from 1970-01-01 to 2049-12-31 because the guest realtime clock is zero until an RTC source is
wired in. It also creates an unrelated untrusted CA and a server certificate under that CA for the
negative chain case. `verify.sh` checks:

* trusted CA + trusted server: success;
* trusted CA + IP SAN `10.0.2.2`: success;
* trusted CA + IP SAN `10.0.2.3`: rejection;
* untrusted CA + trusted server: rejection;
* untrusted CA + its own server: success;
* trusted CA + untrusted server: rejection;
* CA validity covers 1970 through 2049;
* server SAN is exactly `IP Address:10.0.2.2`;
* the CA bundle has one certificate and no private key.

The fixture is test-only. Its CA key is generated on the fly and is never committed. The T2
`tls-client` qtest uses its own ephemeral `/tls-ca.pem`; the T3 fixture adds the negative cases
that the one-shot handshake cannot express without a second target boot.

## CA bundle update path

1. Refresh the host `ca-certificates` package (the image builder owns that input; the build does
   not download).
2. Run `sh addons/tls/certs/update-bundle.sh --check` and record the source path, certificate
   count, SHA-256 and first subject.
3. Install with `--stage` or `--install <image-root>`; the target path is
   `/addons/etc/ssl/certs/ca-bundle.pem`.
4. Validate the staged bundle:
   * the certificate count is stable and non-zero;
   * no `BEGIN ... PRIVATE KEY` block is present;
   * `openssl x509 -in <bundle> -noout -subject` shows the expected first anchor;
   * `sh tests/tls/certs/verify.sh` passes;
   * `cargo test -p raven-tls` and the `tls-client` qtest still pass;
   * `make check-fast` passes.
5. Record the bundle source and version in the ticket/report. An OpenSSL update follows
   `docs/tls.md` separately; a CA update does not change the add-on binaries.
6. Do not commit a production CA bundle to this repository. If a future ticket does, it needs a
   `PROVENANCE.toml` import and the bundle's licence (the common Mozilla/NSS bundle is MPL-2.0,
   tier B; keep it in a separate file and do not edit the certificates).

Rotation is a replace-and-reboot operation in the current image model. The bundle is a file on
the initfs/add-on prefix, so the next image installs the new anchors atomically with the rest of
the image. There is no separate `update-ca-certificates` database or per-anchor enable/disable
list yet.

## Limitations and open risks

* **No CRL or OCSP.** Revocation is not checked; the image must not ship a known-revoked anchor.
  **PROPOSED** follow-up.
* **Trust store wiring.** T3 defines the path and install helper; no product profile installs the
  bundle yet. A consumer image must run `--install` (or copy the staged file) and pass the path to
  `raven_tls::connect`. **PROPOSED**
* **Test-only CA.** The T3 and T2 test CAs are not production anchors. The test server disables
  TLS 1.3 session tickets because of the netd/TCP retransmission race documented in
  `docs/tls-client.md`; that does not change the trust-store policy.
* **IPv4 literals only in the current client.** The wrapper supports DNS names via
  `SSL_set1_host`, but the target client uses `10.0.2.2` and has no resolver. **UNVERIFIED** for
  a DNS name on the target.
* **Entropy.** `rand:` is not cryptographically secure until a real entropy source and PG-52
  `getrandom` land; this is independent of certificate verification but blocks production TLS.
* **No pinning or per-host policy.** The bundle is the only trust input; a caller can pass a
  different file, but the library does not merge a system store or pin a leaf.

## Acceptance

```sh
cargo test -p raven-tls
cargo xtask qtest --case tls-client --arch x86_64
make check-fast
```

The T3 fixture commands are additional evidence, not a replacement for the ticket's acceptance
commands.
