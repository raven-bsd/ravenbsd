<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P44 - POSIX talk

First-party `talk` with a ttyd logged-in terminal registry, independent pty pairs and a local
two-peer transcript, plus the bounded `RVTALK/1` protocol and split-screen model.

## Method

* `ttyd` records each logged-in terminal as `(user, tty path, session, writable)`. A peer registers
  with `OPEN ttyd:peer/<session>/<user>/<tty>`; the handle stays open for the login lifetime and
  closing it (or the session) unregisters the terminal. `OPEN ttyd:who/<user>[/<tty>]` returns a
  lookup handle; `CALL OP_PEER_LOOKUP` replies with the tty path and writable state. `SET_WRITE`
  keeps the registry's writable bit in sync with the session.
* `ptyd` now keeps a table of independent pairs. `pty:master`/`pty:slave` remain pair `0`;
  `pty:master/<id>`/`pty:slave/<id>` select a second pair. Each pair has its own discipline,
  queues and foreground group.
* `talk -l address [terminal]` registers the local user's terminal (default
  `pty:<session>/slave`) and waits for an invitation. `talk address [terminal]` opens its own
  session, registers `pty:<session>/master`, looks the recipient up in ttyd, refuses a
  non-writable terminal, and connects to the complementary pty endpoint.
* The bounded `RVTALK/1` protocol has `INVITE`, `ACCEPT`, `DECLINE`, `DATA`, `ALERT`, `END` and
  `BYE` frames. Text is percent-escaped, every line is limited to 1024 bytes, text to 256 bytes
  and addresses/terminals to 64 bytes.
* The state machine implements the POSIX invitation, accept/decline, simultaneous forwarding,
  explicit EOF, explicit termination and exit statuses. Once one side ends, the other can only
  exit; both sides terminate explicitly. The split-screen helpers emit the labelled panes and
  refresh escape. `Ctrl-L` refreshes, `Ctrl-G` alerts, `Ctrl-D` sends `END`, `Ctrl-C` sends
  `BYE` and exits 0.
* The target wrapper reads the local console through `tty:console` on a reader thread and switches
  the pty transport to raw/no-echo. It never acquires pids, waits or signals (DR-46).

## Tool matrix

| Surface | Covered | Tests |
|---|---|---:|
| ttyd registry | register, user/terminal lookup, duplicate replacement, writable by record/session, session cleanup | 4 lib |
| ptyd pairs | pair 0 compatibility, independent queues and settings for pair 1 | 1 lib |
| talk parser | `user[@host]`, listener address, terminal, `-l`, usage and bounds | 5 lib |
| talk wire | all frames, escaping, UTF-8, malformed and oversized input | 2 lib |
| talk session | invite/accept/decline, both-way forwarding, alert, refresh, EOF/BYE, out-of-order events | 7 lib |
| talk UI | invitation text, split panes, refresh, sanitisation | 3 lib |
| loopback | full encoded two-endpoint transcript | 1 lib + 1 host |
| two-peer path | ttyd registry lookup plus pair-1 pty transcript | 1 host |
| CLI | usage, network rejection, missing registry, missing listener transport | 4 host |

The host `talk` suite is 23 tests (17 library + 6 command-line). The registry and pty modules add
their own host tests.

## Exact unsupported cases

| Case | Detail |
|---|---|
| network `user@host` / `talkd` | not implemented; non-`localhost` hosts are rejected before a session starts |
| `mesg n` permission | no `mesg` utility; the registry carries the writable bit, but there is no command-line toggle |
| extended `stty iexten` controls | only printable ASCII, space, alert, refresh, EOF and interrupt are handled |
| two console users | one `tty:console` UI exists; two peers can have independent pty transports, but both local UIs share the console |

## Acceptance

```
sh tests/posix/utility/run-talk.sh
cargo nextest run -p talk
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

The runner reports `1 pass, 0 fail` with 8 cases. `docs/posix/evidence/P4-talk.toml` claims
`util.talk` only because the two-peer registry + pty transcript passes. No board claim.
