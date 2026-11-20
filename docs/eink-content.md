<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# E-Ink content contract (K201)

Date: 2026-11-20. Ticket: K201. Owner: Otto. Follow-up: E4 (panel), E5 (photo).

## Purpose

The E-Ink demo displays a fetched insult instead of the static "RavenBSD E-Ink" text. This
document fixes the URL/body contract and the deterministic test server; it does not claim a panel
or hardware display (E4) or a photo pipeline (E5).

## Test server (host fixture)

`tests/fixtures/insult-server/` is a Python-stdlib server bound to `127.0.0.1:18081`; QEMU user
networking exposes it to the guest as `10.0.2.2:18081`. It never uses the external network and
exits after 8 requests or its idle timeout.

| Request | Response |
|---|---|
| `GET /insult` | `200`, `Content-Type: text/plain; charset=utf-8`, exact body |
| `GET /insult-final` | `200`, same body (redirect target) |
| `GET /redirect` | `302`, `Location: /insult-final`, empty body |
| `GET /missing` | `404`, body `not found\n` |

Exact body bytes (one line plus newline):

```
Your shell is so slow it still waits for the loader.
```

The body is not hard-coded in the shell profile or display code; `insult.txt` is the server's
source of truth, and the qtest spells the same bytes only as the expected server response.

## Guest proof (`eink-insult`)

The qtest logs in to the release image and runs:

```
curl -s http://10.0.2.2:18081/insult                       # prints the body
curl -s -o /tmp/insult-curl.txt http://10.0.2.2:18081/insult
wget -q -O /tmp/insult-wget.txt http://10.0.2.2:18081/insult
cmp /tmp/insult-curl.txt /tmp/insult-wget.txt && echo cmp-ok
curl -s -o /dev/null -w '%{http_code}\n' http://10.0.2.2:18081/missing   # 404
curl -s -f http://10.0.2.2:18081/missing; echo curl-f:$?                  # 22
curl -s -L http://10.0.2.2:18081/redirect                   # follows to the body
```

`curl`/`wget` come from K199; until that ticket lands the case carries
`skip = "needs K199 curl/wget in the release image"`. No external server, no fake socket, no
panel write.

## E4 handoff

E4 fetches the same contract with the K199 clients (or a first-party HTTP client) and renders the
body on the panel. The URL is configuration, not code: the test uses
`http://10.0.2.2:18081/insult`; a production/demo target supplies its own host. The contract is
"one plain-text line, UTF-8, newline-terminated, `text/plain`"; E4 must not rewrite the fetched
bytes before display, and a failed fetch (`curl -f` nonzero, 404) must surface as a visible error
rather than a stale page.

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| Content server | no server to copy | no donor code needed | host HTTP test outcome only | first-party deterministic fixture |
| Fetch client | E3/E3a first-party | no code | curl/wget outcome only | K199 clients; no donor code |
| Display | not applicable | not applicable | not applicable | E4 panel; this ticket only proves content |

No donor code is copied.
