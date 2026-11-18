<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Online fetch for the Pi 4 E-Ink demo: HTTP core and TLS decision

Status: **decision RECOMMENDED; HTTP core implemented and host-tested (OBSERVED).** A Pi 4
network run, a live HTTP source and any HTTPS path are **UNVERIFIED** by this ticket. E3b owns the
service wiring after the owner confirms the source and the TLS boundary.

## 1. Decision summary

| Question | Decision | Evidence |
|---|---|---|
| What does 0.1.0 fetch? | A small **plain-HTTP/1.1 GET** document from an owner-controlled endpoint (LAN or a public HTTP smoke endpoint); the E-Ink service renders the returned bytes | Ticket E3; `crates/raven-http` tests (OBSERVED) |
| Is HTTPS required for the demo? | **No**, provided the demo source is an HTTP endpoint. HTTPS-only public feeds are an E3b/add-on follow-up, not a 0.1.0 base requirement | This document, sections 2-5 |
| Which TLS path if HTTPS is wanted? | The existing **owner-approved OpenSSL 3.5 C add-on** (`addons/tls/openssl`, wrapped by `crates/raven-tls`) in a **separate add-on process**, never linked into the C-free base service | DR-45, `docs/tls.md`, `docs/tls-client.md` |
| Is a pure-Rust TLS accepted? | **No** for this path: DR-45 says TLS is never implemented in-house and no Rust TLS implementation is used; the practical rustls providers also pull C/assembly or an unaudited provider | DR-45; section 4 |
| When is E3 BLOCKED? | If the owner requires HTTPS to a public source **and** rejects the separate OpenSSL add-on process (or requires a self-written Rust TLS). Then there is no acceptable in-base TLS and E3b/E3 stops with that exact decision | Ticket contract; section 15 |

The base scaffold stays C-free. Plain HTTP is not a security claim: it is a deliberate 0.1.0 demo
scope for an owner-controlled endpoint, with the TLS add-on boundary defined before any public
credentialed or tamper-sensitive source is used.

## 2. Content sources

The E-Ink use case needs a small page on a schedule, not a browser. Candidate sources:

| Class | Example | Transport | 0.1.0 use |
|---|---|---|---|
| Owner dashboard | A tiny text/plain or text/html page served on the LAN or a VPS | HTTP or HTTPS | **Recommended**: HTTP endpoint the owner controls; deterministic demo |
| Public HTTP smoke | `http://example.com/` or another HTTP-only test page | HTTP | Connectivity smoke test only; not content |
| Weather/news/RSS APIs | Public feeds and JSON APIs | HTTPS in practice | Deferred to E3b plus the OpenSSL add-on process |
| Local file/template | `file:` or initfs page | none | Fallback last-good page if the network is down |

The service should allowlist one configured host/path in 0.1.0, cap the body (default 1 MiB, the
core default), accept `text/plain` and a small `text/html` subset (or convert to text in E3b), and
never fetch user-supplied URLs from the panel.

## 3. HTTP/1.1 requirements kept in `raven-http`

The implemented core is deliberately small:

* One method: `GET`. No request body, no `Expect: 100-continue`, no authentication, no cookies,
  no proxy, no decompression.
* Request line, `Host`, `User-Agent`, `Accept`, `Accept-Encoding: identity`, `Connection: close`.
* Response: status line, case-insensitive header lookup, `Content-Length`, `Transfer-Encoding:
  chunked` (extension and trailer tolerant), and close-delimited bodies.
* Redirects 301/302/303/307/308 with a bounded budget and a credential-drop policy.
* A total timeout; connect/write/read receive the remaining milliseconds.
* Bounded status line, header block, header count, chunk line and body. Malformed input is an
  error, never a panic.
* Host/path/header injection is rejected before a byte reaches the transport.

HTTPS adds a TLS handshake, SNI, certificate-chain and hostname verification, a CA bundle, random
seeding and a much larger CVE surface. None of that belongs in the HTTP parser or in this crate.

## 4. Pure-Rust TLS options under stable Rust and the no-C/no-asm rules

RavenBSD hard rule 2 allows assembly only through `core::arch::global_asm!`/`asm!`; no C or C++ in
the scaffold. DR-45 is stricter for TLS: **never implement TLS in-house**, use upstream OpenSSL
3.x as a C add-on, and no Rust TLS implementation for this path. The options therefore fall into
three groups:

| Option | Licence (verify before any import) | C/assembly | Assessment |
|---|---|---|---|
| `rustls` with the default `ring` provider | rustls: permissive (Apache-2.0/MIT/ISC per upstream); ring: permissive | **ring contains C and perlasm-generated assembly** | Rejected: violates the no-C/no-asm base rule and adds a second crypto stack beside the owner-chosen OpenSSL |
| `rustls` with `aws-lc-rs` | rustls permissive; aws-lc-rs permissive | **C and assembly** | Rejected for the same reason |
| `rustls` with a RustCrypto provider | rustls permissive; provider crates permissive (mostly Apache-2.0/MIT) | Pure Rust | Rejected by DR-45: not the owner-approved TLS path, not the audited provider, and it would put a second TLS implementation in the base; "pure Rust" does not make an unaudited TLS stack acceptable |
| Embedded/minimal TLS crates (for example a TLS 1.3-only client) | permissive in most cases | Pure Rust or small asm | Rejected: incomplete protocol/audit story, no certificate/PKI policy matching OpenSSL, and still a Rust TLS implementation |
| OpenSSL 3.5 C add-on (`addons/tls/openssl`, existing) | Apache-2.0 (add-on, not base) | C, deliberately outside the scaffold | **Accepted for the HTTPS follow-up**, in a separate process; DR-45/CVE tracking already documented in `docs/tls.md` |
| `crates/raven-tls` thin wrapper | BSD-3-Clause wrapper over the add-on | links the add-on only in an add-on consumer | **Accepted as the E3b integration seam**; not linked into `raven-http`, `einkd` base or any C-free service |

The licence cells are a checklist for a future import, not an approval. No TLS crate is added by
E3, so none of those licences or donor revisions enter `Cargo.lock` here.

**Consequence:** if the owner wants HTTPS in the 0.1.0 demo, the path is an add-on process (for
example a small `fetchd`/`tlsd` that speaks HTTP over `raven-tls`) and `einkd` talks to it through a
scheme. That keeps the base `einkd` and `raven-http` C-free and avoids a second TLS stack.

## 5. DNS through `resolvd`

`resolvd` already owns DHCP/static DNS and the `resolv:` scheme. The fetch service must resolve in
this order:

1. Read the current nameserver list from `/etc/resolv.conf` (or use the live DHCP lease through
   `resolvd`; the scheme already handles both).
2. Open `resolv:` and send one `Query` with `QTYPE_A` for the host; parse the fixed reply
   (`STATUS_OK`, records, TTL; `NXDOMAIN` is a terminal failure).
3. Cache the A record for the returned TTL (bounded, for example 60 s..1 h), and use the first
   address for `net:tcp/<ipv4>:<port>`.
4. `QTYPE_AAAA` is available in the resolver, but the current `net:` path parser is IPv4-only, so
   0.1.0 uses A and fails closed if only AAAA exists. IPv6 transport is a later ticket.
5. DNS timeout and retry are separate from the HTTP total timeout; a failed lookup is reported as a
   DNS failure, not as an HTTP status.

The HTTP core never resolves names. It receives an already-connected `Transport`, which keeps
`resolvd` policy, cache and IPv6 decisions outside the parser.

## 6. URL semantics

* Absolute URLs only: `http://host[:port]/path?query`. `https://` is parsed so a redirect can be
  recognized, but the core returns `UnsupportedScheme` instead of opening it.
* Host: DNS name, IPv4 literal, or bracketed IPv6 literal; lower-cased; userinfo (`user@host`) is
  rejected; a missing host is rejected; port 0 and non-IPv4/IANA-valid ports are rejected.
* Default ports: 80 for HTTP, 443 for HTTPS; `Host` omits the default port.
* Path: origin-form, always starts with `/`; the query is preserved; the fragment is removed; the
  request line never contains raw spaces or control bytes; percent-encoding is preserved.
* Maximum URL length: 2048 bytes. Longer values are rejected before building a request.

## 7. Redirect policy

The core follows 301, 302, 303, 307 and 308 with `GET` preserved (a GET has no body to replay).

* `Location` may be absolute (`http://other/x`), network-path (`//other/x`), absolute-path
  (`/x`), relative (`x`, `../x`) or query-only (`?q=1`); it is resolved against the current URL
  with RFC 3986 dot-segment removal.
* Default budget is 5 redirects; the builder can lower it. Exhaustion is `TooManyRedirects`,
  never an infinite loop.
* A redirect to `https://` is `UnsupportedScheme`; the service may hand the URL to the TLS add-on
  process. It must not silently downgrade or fetch the plain-HTTP variant.
* A redirect status without `Location` is `MissingLocation`.
* On a **cross-origin** redirect (host or port change), `Authorization`, `Cookie` and
  `Proxy-Authorization` are dropped. Same-origin redirects keep them (0.1.0 sends none).
* The service additionally enforces a configured source allowlist; the core's policy is about
  correctness, not a substitute for the service's trust boundary.

## 8. Timeouts and retries

* Total request timeout: 10 s by default, configurable per request. Zero means immediate failure.
* The core computes the deadline once, checks the monotonic clock before every connect/write/read
  and passes the remaining milliseconds to the transport. A transport that blocks longer than the
  remaining budget is a transport bug; the clock check still stops the next operation.
* DNS has its own bounded exchange inside `resolvd`; the service adds one overall deadline around
  DNS + connect + fetch.
* E3b policy: one fetch attempt per schedule; on failure keep the last-good frame and back off
  1, 2, 5, 15 minutes (capped), with jitter. Do not retry aggressively or cycle the panel.

## 9. Body framing and security limits

| Limit | Value | Why |
|---|---|---|
| Status line | 512 bytes | Bounded parser, no unbounded line allocation |
| One header line | 8 KiB | Reject absurd values before the header block |
| Header block | 16 KiB | Bounded total work |
| Header count | 64 | Bounded allocation |
| Chunk-size line | 128 bytes | Bound extensions/garbage |
| Body | 1 MiB default | E-Ink page scope; builder can lower/raise deliberately |
| URL | 2048 bytes | Request-line and memory bound |

Additional rules:

* `Transfer-Encoding: chunked` plus `Content-Length` is rejected (request-smuggling boundary).
* Duplicate `Content-Length` values must agree; disagreement is rejected. A value above the body
  limit is `BodyLimit` before allocation.
* Only `identity` and `chunked` transfer codings are accepted; anything else is
  `InvalidResponse`.
* Obs-fold (a header line starting with space/tab) is rejected; header names must be RFC tokens;
  values may not contain CR, LF or other control bytes.
* Chunk sizes must be hexadecimal and the chunk terminator must be CRLF; trailers are bounded and
  discarded.
* 1xx responses are skipped until a final status; 101 (protocol switch) is rejected.
* 204/304 have no body. A close-delimited body ends at EOF; a truncated `Content-Length` or chunk
  is `Truncated`.
* No `Content-Encoding` decompression in 0.1.0; the request asks for `identity`, and a compressed
  response is returned as bytes (the service can reject it). No cookies, authentication or proxy
  support.

## 10. Cache and refresh policy (E3b service)

The HTTP core is stateless. The fetching service owns:

* `ETag`/`If-None-Match` and `Last-Modified`/`If-Modified-Since`; a `304` keeps the last-good frame
  and does not refresh the panel.
* `Cache-Control: max-age`/`no-store`; default refresh 15 minutes, minimum 5 minutes, with jitter
  to avoid a thundering herd. `no-store` is honoured for the in-memory page.
* Last-good frame persistence across service restarts (the panel keeps the image anyway).
* A scheduled refresh waits for NTP-corrected wall time for clocked content, but the first frame
  may use the persisted last-good page without blocking login or time sync.
* A failed fetch never replaces the frame with an error page; log the class (DNS, connect, timeout,
  HTTP 4xx/5xx, body limit, malformed response) and retry with backoff.

## 11. Failure behavior and security posture

* Plain HTTP is unauthenticated and tamperable. Use it only for a demo page the owner controls;
  do not send credentials or personal data over it.
* HTTPS, when added, must verify the chain and hostname (OpenSSL defaults, no unverified mode) and
  use the pinned CA bundle from `docs/tls-trust.md`.
* The service allowlists the source host/path, rejects redirects outside the allowlist, caps the
  body and never exposes a user-controlled URL to the panel.
* TLS is never silently downgraded; an `https` URL is either handed to the add-on or fails.
* No live-network claim is made by E3. The host tests use fixed response bytes only.

## 12. Service boundary (E3b)

```
einkd
  -> resolv: (A/AAAA via resolvd)       DNS, TTL, fail closed
  -> net:tcp/<ipv4>:<port>              connected Transport
  -> raven-http Client                   GET, headers, body, redirects, timeout
  -> renderer/Canvas                last-good page, refresh schedule
  -> optional fetchd/tlsd add-on         HTTPS only, OpenSSL + raven-tls (DR-45)
```

`raven-http` has no sockets, no clock syscall and no allocator policy beyond `alloc`. The service
supplies `Transport` and `Clock`, so the core is host-testable and the network/TLS capabilities
stay in the service process.

## 13. DR-57 / DR-52 donor reconciliation

Donor revisions from `ravenbsd-resources/donors.lock`:
`netbsd 36eddebcc36cb0775aa2962005b9c05419a6ff01`,
`freebsd 133344405f532bd54ff3629a77d4c4e062f01fdb`,
`illumos-gate 586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`,
`linux adc218676eef25575469234709c2d87185ca223a`.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| HTTP response state machine | No HTTP client in the base OS; libcurl/wget are add-ons. The useful idea is a service/transport boundary, not a parser | `netbsd/external/bsd/fetch/dist/libfetch/http.c` (BSD-3-Clause, NetBSD rev 1.14 / 2026-04-16) and its FreeBSD lineage: status line, header match, `Content-Length`, chunked reader, 301/302/303/307/308 | curl/wget behaviour: status/headers/body, redirects, chunked; read-only reference, no code | Independent Rust implementation. The donor is socket/SSL-coupled; the ticket needs a `Transport`/`Clock` seam plus explicit security limits. The state machine and edge cases were taken from the donor; no C code, comments or table layouts were copied. |
| Chunked decoding | N/A | `http.c` `http_new_chunk`/`http_fillbuf`: hex size, optional `;extension`, CRLF after each chunk, zero chunk then trailer line | curl's chunked decoder is the behavioural reference | Reimplemented with bounds, hex validation, CRLF strictness and a trailer bound. |
| Header parsing | N/A | `http.c` `http_get_reply`/`http_next_header`: accept `HTTP/1.0`/`1.1`, three-digit status, case-insensitive name match, trim trailing whitespace | curl header folding/duplicate handling | Reimplemented with token validation, obs-fold rejection, duplicate `Content-Length` policy and the TE+CL smuggling rejection. |
| Redirect policy | N/A | `http.c` redirect status set and `Location` handling | curl's relative-URL resolution and credential scoping | Reimplemented with RFC 3986 dot-segment removal, a bounded budget, cross-origin credential drop and an `https` hand-off error. |
| TLS | OpenSSL is the system crypto on Illumos; no in-house TLS | NetBSD libfetch uses OpenSSL; the add-on path is documented in `docs/tls.md` | OpenSSL/curl use OpenSSL; read-only | Use the existing OpenSSL 3.5 add-on in a separate process (DR-45); no Rust TLS in the base. |
| Tests | N/A | Donor has no host-only fixed-response suite for our seam | N/A | Fixed mock `Transport`/`Clock` tests; no live network, no QEMU. |

The source file is **not copied or transpiled** here: NetBSD libfetch is a socket/SSL/`FILE *`
stack, and a literal port would carry C lifecycle and OpenSSL branches into a crate that must be
`no_std`, allocation-bounded and testable with a byte-stream seam. The reconciliation records the
behavioural paths read and the independently written Rust. If a later ticket ports a permissive
libfetch file literally, it must add the upstream notice and `PROVENANCE.toml`.

## 14. Open inputs for the owner / E3b

1. The exact 0.1.0 URL and whether an HTTP-only endpoint is acceptable for the demo.
2. The source allowlist and refresh interval (default 15 min, minimum 5 min).
3. Content rendering policy: text/plain only, or a restricted HTML-to-text step; image support is
   a renderer question, not an HTTP one.
4. Whether HTTPS is required in 0.1.0. If yes, approve the separate OpenSSL add-on process and its
   CA bundle, or accept the BLOCKED condition in section 15.
5. Whether E3b may add a `fetchd`/`tlsd` service and a scheme for it (new Owns path).

## 15. Blocked condition

E3 is **not blocked** for the recommended HTTP demo. It becomes BLOCKED only if the owner requires
HTTPS to a public source **and** refuses the existing OpenSSL 3.5 add-on process (or requires a
self-written Rust TLS implementation). In that case there is no acceptable pure-Rust TLS under
stable Rust, the no-C/no-asm scaffold rule and DR-45, and E3b must stop with that decision rather
than fake TLS or link C into the base.
