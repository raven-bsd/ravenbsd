<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# HTTP socket and DNS transport

Status: **host-tested adapter and local QEMU fetch gate (OBSERVED); external network and Pi 4
behaviour UNVERIFIED.** E3a implements the `raven_http::Transport` seam over the platform `net:`
socket scheme and the `resolv:` DNS scheme. The local gate uses a one-shot host server, not the
external owner URL. It does not open a device, link C or fake a socket. The E3 decision
(`docs/design/eink-fetch.md`) remains in force: plain HTTP for 0.1.0 and the existing OpenSSL
add-on process for any HTTPS source.

## 1. Purpose and boundary

`crates/raven-http` owns the HTTP/1.1 state machine. It needs three things from the platform:

1. resolve a host name;
2. open one TCP connection and send/receive bytes with a bound;
3. close the connection cleanly.

`crates/raven-http-net` supplies those through two host-testable traits:

| Trait | Contract | Host test | Target implementation |
|---|---|---|---|
| `Resolver` | `resolve(host, timeout_ms) -> Resolved { address, ttl_secs }` | `MockResolver` returns fixed IPv4 or a failure | `SchemeResolver` over `resolv:` |
| `Socket` | `connect(Ipv4Addr, port, timeout_ms)`, `send`, `recv`, `close` | `MockSocket` scripts partial reads, timeouts and failures | `SchemeSocket` over `net:` |
| `HttpTransport<S, R>` | implements `raven_http::Transport`: resolve, connect, write, read | drives the real `raven_http::Client` through mocks | `live_transport()` |

The adapter is deliberately below HTTP: it does not parse headers, retry HTTP statuses or know
about redirects. `raven_http::Client` owns those and calls `connect` again for a new host.

## 2. Syscall / scheme mapping

RavenBSD 0.1.0 has no completed libc lazy-socket path. The platform TCP API is the `net:` scheme
served by `netd`; the adapter uses S11 scheme operations, not raw device handles.

| Operation | Scheme mapping | Notes |
|---|---|---|
| open TCP socket | `client.open("net:tcp/<ipv4>:<port>", READ|WRITE)` | `netd` parses `net:tcp/...` and creates the socket |
| connect | `CALL` `SocketRequest::Call(Call::Connect(Endpoint { ip, port }))` | `Response::Err(EAGAIN)` means “in progress”; retry with a bounded sleep |
| send | `client.write(handle, 0, bytes)` | returns bytes accepted; may be partial |
| receive | `client.read(handle, 0, buffer)` | `Ok(0)` is an orderly close; `EAGAIN` means no data yet |
| close | `client.close(handle)` | idempotent at the adapter level |

`SchemeSocket` polls `EAGAIN` every 5 ms and decrements the caller's remaining budget. A zero
budget fails immediately with `SocketError::Timeout`. `HttpTransport` closes the previous socket
before a redirect's new `connect`, and its `Drop` closes the last one.

## 3. `resolvd` protocol

`SchemeResolver` opens `resolv:` with `READ` and sends one `CALL` containing the fixed `resolv:`
query; it does not reimplement the DNS wire loop.

| Field | Encoding |
|---|---|
| version | `u16` little-endian, `1` |
| qtype | `u8`, `1` (A) |
| name length | `u16` little-endian |
| name | ASCII labels, at most 253 bytes |
| reply header | version, status, qtype, record count |
| reply record | qtype, `u32` TTL, four IPv4 octets |

Status `0` with at least one A record yields the first IPv4 address. `NXDOMAIN`, an empty answer,
a bad version/type/length or a malformed reply maps to `ResolveError::NotFound`/`Io`. A resolution
timeout (when `resolvd` returns `ETIMEDOUT`) maps to `ResolveError::Timeout`.

IPv4 literals are returned without a DNS query and carry TTL zero. `raven_http` URLs may carry a
bracketed IPv6 literal, but the current `net:` path is IPv4-only; an IPv6-only name fails closed
and is a later transport ticket. The adapter itself does not cache DNS results; it returns
`Resolved { address, ttl_secs }` so the E3b service owns the TTL-bounded cache and refresh policy.

`timeout_ms` is the remaining HTTP budget. The `resolv:` exchange itself is bounded by `resolvd`'s
resolver; the synchronous scheme `CALL` cannot be preempted from this adapter, so E3b must keep an
outer deadline around DNS + fetch and terminate/abandon the fetch on expiry.

## 4. Privilege and service dependencies

* `net:` is provided by `netd`; `resolv:` is provided by `resolvd`. Both are ordinary service
  schemes, not device capabilities.
* `einkd` (E3b) declares `needs = ["scheme:net", "scheme:resolv"]` and `provides` its own scheme;
  it needs no `root`, `platform-root` or DMA/IRQ grant.
* The adapter never opens `pci:`, `disk:` or a device node. `netd`/`resolvd` own the privileged
  setup (DHCP lease, rDNS configuration, DNS sockets) and the adapter sees only a scheme handle.
* HTTPS, when added, is a **separate add-on process** with the OpenSSL libraries. `einkd` and
  `raven-http-net` stay C-free and do not gain a TLS capability.

## 5. Error mapping

| Layer | Error | Maps to |
|---|---|---|
| `Resolver` | `NotFound`, `Io` | `raven_http::TransportError::Io` |
| `Resolver` | `Timeout` | `raven_http::TransportError::Timeout` |
| `Socket` | `Io` | `raven_http::TransportError::Io` |
| `Socket` | `Timeout` | `raven_http::TransportError::Timeout` |
| `raven_http` | transport/timeout | its own `Error::Transport`/`Error::Timeout` |
| parser | body above cap | `raven_http::Error::BodyLimit` (adapter does not buffer bodies) |

A partial `send` is legal: `raven_http` writes the remainder. A partial `recv` is legal: its
`Reader` buffers and continues. A zero `recv` is an orderly close for close-delimited bodies; an
early zero for a declared `Content-Length` is `Error::Truncated` from the HTTP core.

## 6. Redirect host changes

`raven_http::Client` calls `HttpTransport::connect(host, port, remaining)` for every redirect
target. The adapter:

1. closes the current `net:` handle (if any);
2. skips DNS for an IPv4 literal, otherwise calls `resolv:`;
3. opens a new `net:tcp/<ipv4>:<port>` handle and connects;
4. lets `raven_http` re-encode the request line and `Host` header for the new target.

Cross-origin credential headers were already dropped by `raven_http`; the adapter never sees them.
A redirect to `https://` is rejected by the core before `connect`, so the adapter is never asked to
open a TLS target. The service still enforces its source allowlist.

## 7. TLS add-on boundary

E3a links no TLS and no C. The E3 decision and DR-45 keep TLS in the pinned OpenSSL 3.5 add-on
(`addons/tls/openssl`, `crates/raven-tls`) in a separate process. If an `https://` URL is required,
E3b routes it to that add-on process; `raven-http-net` must not grow a TLS feature, a second crypto
stack or an unverified mode. The base image remains C-free.

## 8. Live-test plan for the owner URL

The **local** gate is implemented and OBSERVED:

```
cargo xtask qtest --case http-fetch --arch x86_64
```

It boots the release image with a patched initfs containing the static `/bin/http-fetch` probe,
against a one-shot host server on `127.0.0.1:18080` exposed as `10.0.2.2` by QEMU user networking.
The probe gets `http://10.0.2.2:18080/known.txt` through `raven_http` + `raven_http_net`, verifies
status 200, `Content-Length: 15` and the exact body `known-http-body`, prints one marker per fact
and exits 0. The transcript is `build/qtest/http-fetch.log`; the host server log is
`build/fixtures/http-fetch/server.log`. The server accepts one connection and exits; it is not a
service. No external network, TLS or fake socket is involved.

The **external owner/Pi 4** steps remain:

1. Boot the release image on QEMU x86_64 and on the Pi 4 with `netd` and `resolvd` up; record
   `ifconfig`/`route` and the DHCP or static address.
2. Set the owner URL (HTTP only for 0.1.0) and the allowlist in the service configuration.
3. Resolve the host with a `resolv:` probe; record the A answer and TTL.
4. Run the E3b fetch probe against the owner URL; assert the HTTP status, a bounded body and a
   known marker; record the full serial transcript. No synthetic “success” line: the transcript
   must show the real request/response.
5. Repeat with the service configured to the last-good page and with the network down: the panel
   must keep the last frame and the fetch must not hang past its deadline.
6. Record the owner URL, allowlist, refresh interval and HTTP-only approval in the report. If the
   owner requires HTTPS, record the add-on process and CA bundle instead; do not link C into the
   base.

## 9. DR-57 / DR-52 donor reconciliation

Revisions from `ravenbsd-resources/donors.lock`: `netbsd 36eddebcc36cb0775aa2962005b9c05419a6ff01`,
`freebsd 133344405f532bd54ff3629a77d4c4e062f01fdb`, `illumos-gate
586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`, `linux adc218676eef25575469234709c2d87185ca223a`.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| TCP connect/read/write semantics | Illumos socket library is not present in this sparse checkout; POSIX socket semantics apply. The useful idea is a per-socket state machine and a close that unblocks the peer | FreeBSD `lib/libc/sys/connect.c` (BSD-2-Clause, rev 1.0/2014) and `lib/libc/sys/recv.c` (BSD-3-Clause); NetBSD `lib/libc/net/recv.c` (BSD-3-Clause) — connect/recv are thin syscall wrappers, with cancellation and partial-read semantics | POSIX/Linux `connect`/`recv` behaviour is the read-only reference | Independent Rust over `net:`; the adapter keeps partial send/recv, zero-close and bounded EAGAIN polling. No C code or wrappers copied |
| DNS resolution | No in-base HTTP client; resolver service design only | NetBSD `lib/libc/resolv/res_send.c` (ISC + BSD-3-Clause, rev 1.30) and `res_query.c` (rev 1.18): server retry, timeout, A/AAAA and NXDOMAIN | POSIX resolver behaviour only | Delegate to `resolvd`; the adapter implements only the fixed `resolv:` control query. No resolver code copied, no in-house DNS wire loop |
| Scheme mapping | N/A (Raven-specific S11) | N/A | N/A | First-party `net:`/`resolv:` mapping in `raven-http-net`; no donor |
| HTTP transport | N/A | E3 donor table for `libfetch/http.c`; no new HTTP parser here | curl behaviour only | Use `raven-http`; the adapter is below HTTP |
| TLS | OpenSSL/system crypto; no in-house TLS | libfetch uses OpenSSL; `docs/tls.md` | OpenSSL/curl (read-only) | Separate OpenSSL add-on process (DR-45); no TLS here |
| Tests | N/A | No host-only fixed-response suite for this seam | N/A | Mock `Socket`/`Resolver` + real `raven_http::Client`; no live socket success claimed |

The donor files are socket/syscall and resolver service code; a literal port would pull C
cancellation, `FILE *` and DNS wire state into a crate whose job is a four-operation Rust trait.
The behavioural paths were read and the Rust implementation is original and bounded. No
`PROVENANCE.toml` import is added.

## 10. Remaining owner inputs

* The exact 0.1.0 URL and allowlist.
* Whether HTTP-only is accepted for the demo (E3 recommendation) or the OpenSSL add-on process is
  required; if the latter, its CA bundle and process boundary.
* Refresh interval (E3 default: 15 min, minimum 5 min) and content rendering policy.
* The Pi 4 live-run window and the transcript location.
