# Fetch tools: simple curl and wget

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

Status: **OBSERVED** by the K199 host tests and the `fetch-http`/`fetch-https` qtests. The base
build is HTTP-only and C-free; the `tls` add-on build links `raven-tls` and the owner-approved
OpenSSL 3.x add-on. No Pi 4 board claim is made here.

## What exists

| Tool | Base build | `tls` add-on build |
|---|---|---|
| `curl URL` | Writes the response body to stdout | Same, plus verified HTTPS |
| `wget URL` | Saves to the URL basename (or `-O FILE`) and prints one summary | Same, plus verified HTTPS |

Both tools share `raven-http`'s bounded GET core and `raven-http-net`'s `net:`/`resolv:`
transport in the base build. The add-on build uses a small `net:`/`resolv:` transport that hands
the connected stream to `raven-tls`, which uses upstream OpenSSL for the handshake and
certificate/hostname verification. There is no rustls/ring, no custom crypto, and no unverified
fallback.

## Supported options

### curl

| Option | Behaviour |
|---|---|
| `-o FILE` | Write the body to `FILE`; `-o -` is stdout. Overwrites an existing file. |
| `-s` | Suppress non-body diagnostics (the body still goes to stdout). |
| `-f` | Treat HTTP status >= 400 as failure (exit 22). Without `-f`, the body is printed. |
| `-I` | Print the status line and response headers. This is a GET whose body is discarded; the core has no HEAD method. |
| `-L` | Follow up to five redirects. Without `-L` the redirect budget is zero. |
| `--max-time SECS` | Total request deadline; a positive integer. The default is 10 s. |
| `-H 'Name: value'` | Add one request header. Reserved hop-by-hop/host headers are rejected by the core. |
| `--cacert FILE` | TLS add-on build only: trust exactly this PEM bundle instead of the default `/addons/etc/ssl/certs/ca-bundle.pem`. |
| `-V`, `--version` | Print `curl <version> (RavenBSD)` and exit 0. |

### wget

| Option | Behaviour |
|---|---|
| `-O FILE` | Write to `FILE`; `-O -` is stdout. Overwrites an existing file. |
| `-q` | Suppress the summary and diagnostics. |
| `--max-time SECS` | Total request deadline; default 10 s. |
| `--cacert FILE` | TLS add-on build only; same trust semantics as curl. |
| `-V`, `--version` | Print `wget <version> (RavenBSD)` and exit 0. |

`wget` always follows a bounded redirect set. The default output name is the last URL path
segment; an empty path or trailing slash becomes `index.html`; `.` and `..` are rejected. There is
no recursive spidering, no shell execution, no resume, no POST and no cookie jar. Combined short
flags such as `-sf` are not supported. This is deliberately a small subset, not GNU curl/wget.

## HTTPS and the add-on boundary

The base build does not link C or OpenSSL. An `https://` URL in the base build prints a clear
error and exits non-zero:

```
curl: https unavailable (EUNAVAILABLE): TLS add-on not linked
wget: https unavailable (EUNAVAILABLE): TLS add-on not linked
```

The add-on build is compiled with `--features tls` for the `x86_64-unknown-ravenbsd` target and
links the static `libssl.a`/`libcrypto.a` from `addons/tls/`. It rewrites
`https://host[:port]/path` to the equivalent `http://host[:port]/path` request for the HTTP core
and lets the transport perform the verified TLS handshake. The `Host` header keeps the explicit
port. `--cacert` selects the PEM bundle; the default is the T3 trust-store path
`/addons/etc/ssl/certs/ca-bundle.pem`. A missing bundle, an untrusted chain or a hostname/IP
mismatch is a verified failure; the tools never retry with a different CA or downgrade to HTTP.

HTTPS redirects are not followed in this build: the transport is bound to the TLS peer selected
by the original URL. HTTP redirects are followed per `-L`/wget policy. This is recorded as a
limitation, not hidden.

## Build and staging

Base tools (the release profile can stage these without C):

```sh
cargo build -p curl -p wget --target x86_64-unknown-none --release
```

Add-on tools (test/HTTPS build; the OpenSSL artifacts are built by `sh addons/tls/build.sh
--build`):

```sh
RUSTUP_TOOLCHAIN=ravenbsd cargo rustc -p curl --bin curl --features tls \
    --target x86_64-unknown-ravenbsd --release -- <the T2 link arguments>
```

The exact link arguments are in `tests/fixtures/fetch/build.sh`, which reuses the T2/T3
OpenSSL/libc link pattern. The script also absorbs two host-toolchain drifts: clang 14's
`<stdatomic.h>` ordering and the current libc's duplicate `tls_compat.c` symbols. Those are
host build workarounds, not product source changes.

A profile fragment is provided at `tests/fixtures/fetch/fetch.d/76-fetch.toml`:

```toml
[[initfs.programs]]
package = "curl"
path = "/bin/curl"

[[initfs.programs]]
package = "wget"
path = "/bin/wget"
```

`profiles/**` is outside K199's Owns list, so copying this file to
`profiles/release.d/76-fetch.toml` (or the K193 UFS profile fragment) is the one-line staging
handoff. The fragment stages the base HTTP tools; the TLS add-on build is a separate artifact and
is staged by the fetch qtests. The release login case does not depend on this fragment.

## Tests and evidence

```sh
cargo nextest run -p curl -p wget -p raven-http
cargo xtask qtest --case fetch-http --arch x86_64
cargo xtask qtest --case fetch-https --arch x86_64
```

`tests/fixtures/fetch/build.sh http` builds the base tools and a multi-request local HTTP server
on `127.0.0.1:18081` (QEMU exposes it as `10.0.2.2`). The `fetch-http` case covers stdout,
`-o`, `wget -O`, the default wget basename under `/tmp`, `404`/`-f`, `-L`, `-I`, `-H`,
`--max-time`, a missing host, the exact insult body
`insult: your code is so slow it missed the TLB shootdown`, and the base-build HTTPS
`EUNAVAILABLE` fallback.

Fixture ports are unique (K219): `fetch-http` uses `18081`, `fetch-installed` uses `18082`, and
`eink-insult` uses `18083`. Each pre stops only its own previous pid, waits for it to exit,
removes the pid file, starts the server, and verifies both the process and the listening socket
with a bounded retry. If the port is still held after the pid stop, a process rooted in this
checkout is stopped as a stale server; a listener from another checkout is a hard failure with
the `ss` transcript, never a cross-tree kill. No fixture kills another fixture's server. The
`/slow` endpoint sleeps 5 s while the `fetch-http` client keeps `--max-time 1`, so
`curl-slow=1` proves the timeout with real margin instead of a fast reply under load.

`build.sh https` builds the add-on tools, generates the T3 test CA/server certificate (valid
1970-2049 for the zero guest clock) and starts a host TLS HTTP server on `24443` backed by
Python's OpenSSL `ssl` module with the T3 cert/key. The `fetch-https` case verifies the exact
body through `curl` and `wget` with `--cacert /tls-ca.pem`, then proves two negative paths:
an unrelated CA fails the handshake and a missing default CA fails before the handshake. The test
server is pinned to TLS 1.2 and sends a proper `close_notify`; this avoids the known netd/TCP
TLS 1.3 ticket/CCS race. The target TLS stack and verification path are unchanged. The host
server log is `build/fixtures/fetch/https-server.log`; QEMU serial logs are under
`build/qtest/`.

## Static shell staging

`/bin` tools in the shell and release profiles are linked **static**. The login shell cannot spawn
an interpreter image, so a dynamic PIE `/bin/curl` or `/bin/wget` fails as `command not found`
even though the file exists. `STATIC_SHELL_PACKAGES` in `xtask/src/initfs.rs` names the packages
built static: `ed`, `man`, `raven-date`, `raven-hostname`, `raven-ps`, `raven-rsh`,
`raven-sleep`, `raven-svcctl`, `raven-uname`, `curl` and `wget`. Everything under `/sbin` and
`/init` is static regardless of the list; `getty` and `login` remain dynamic because `svcd`
starts them through the loader, not the shell.

The release image stages `/bin/curl` and `/bin/wget` from those static builds; the
`fetch-installed` qtest executes `/bin/curl --version`, `/bin/wget --version` and a real local
HTTP fetch from the release shell. K201's test-only initfs overlay may still replace the binaries
for `eink-insult`, but the product staging no longer depends on it. **OBSERVED** by
`fetch-installed`.

## Limits and non-claims

* IPv4 and DNS through `resolvd` are the 0.1.0 path; IPv6 transport is not claimed.
* `-I` is GET+discard, not a HEAD request.
* No decompression, cookies, authentication, proxy, POST, recursive download or resume.
* The base build returns `EUNAVAILABLE` for HTTPS; it is never an unverified download.
* The HTTPS qtests use a local server and the test CA. No external-network or Pi 4 claim is made.
* The release profile staging fragment is a handoff because `profiles/**` is outside Owns.

## Donor reconciliation (DR-52)

Donor revisions: NetBSD `36eddebcc36cb0775aa2962005b9c05419a6ff01`, FreeBSD
`133344405f532bd54ff3629a77d4c4e062f01fdb`, Illumos
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`, Linux
`adc218676eef25575469234709c2d87185ca223a` (read-only).

| Topic | Illumos | BSD file / licence | Linux | Raven decision |
|---|---|---|---|---|
| HTTP GET state machine | No in-base HTTP client; service/transport boundary only | NetBSD `external/bsd/fetch/dist/libfetch/http.c` (BSD-3-Clause, RCS 1.14) and the FreeBSD libfetch lineage: status/header parsing, `Content-Length`, chunked, redirects | curl/wget behaviour only | Use the existing first-party `raven-http`; no donor code copied. |
| CLI surface, output and failures | No direct tool | FreeBSD `usr.bin/fetch/fetch.c` (BSD-2-Clause) and NetBSD `usr.bin/ftp` outcomes; curl/wget outcomes | curl/wget option/exit behaviour only | First-party Rust `curl`/`wget` over the shared core; bounded subset documented; no copy. |
| TLS | OpenSSL/system crypto; no in-house TLS | libfetch uses OpenSSL; T2/T3 trust fixture | OpenSSL/curl outcome only | Existing owner-approved OpenSSL add-on via `raven-tls`; verification on; no rustls/ring/custom crypto. |
| URL/redirect/error handling | Outcome only | libfetch `http.c` redirect statuses and `Location` behaviour | curl relative-URL and exit-code behaviour only | Reimplemented in the HTTP core; bounded budget and explicit `EUNAVAILABLE`; no copy. |
| Tests | N/A | No host-only fixed-response suite for this CLI seam | N/A | First-party local HTTP/TLS servers and fixed bodies; no external network. |

No donor code was copied or translated. `PROVENANCE.toml` is unchanged; the OpenSSL add-on's
existing provenance import remains the only TLS source.

## TLS client output and the add-on libc

The TLS add-on binaries (`curl-tls`/`wget-tls`) are static programs linked against the add-on
sysroot, which carries its own copy of `libc.a`. The fetch fixture now refreshes that copy when
the libc sources are newer: a stale copy made the TLS client run an older fd/stdio/net stack
than the shell and kernel. The shell-spawned add-on child can also have no usable fd 1; the TLS
clients fall back to the base client's `DEBUG_WRITE` console sink so `-I` headers, summaries and
errors are never lost. **OBSERVED** by `fetch-https` (K218).
