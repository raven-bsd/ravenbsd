<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P44 - POSIX talk

First-party `talk` protocol, split-screen and session-state implementation, with host tests and a
local loopback diagnostic. The `util.talk` profile row is **not claimed present**: the image has no
peer-terminal registry or peer transport, so a real two-user session cannot be established.

## Method

* `talk` parses `talk [-l] address [terminal]`, where `address` is `user[@host]`. `-l` is a local
  listener extension; POSIX defines no options.
* The bounded `RVTALK/1` wire protocol has `INVITE`, `ACCEPT`, `DECLINE`, `DATA`, `ALERT`, `END`
  and `BYE` frames. Text fields are percent-escaped, every line is limited to 1024 bytes, text to
  256 bytes and addresses/terminals to 64 bytes.
* The session state machine implements the POSIX invitation, accept/decline, simultaneous
  forwarding, explicit EOF, explicit termination and exit statuses. Once one side ends, the other
  can only exit; both sides must terminate explicitly.
* The split-screen helpers emit the labelled panes and refresh escape. `Ctrl-L` refreshes,
  `Ctrl-G` alerts the peer, `Ctrl-D` sends `END`, `Ctrl-C` sends `BYE` and exits 0. Remote text is
  filtered to printable ASCII and spaces.
* The target wrapper probes `ttyd:session` (`STATE`) so the diagnostic names the local session and
  foreground group, then reports the missing peer registry. It never acquires pids, waits or
  signals (DR-46).
* `--loopback` runs both endpoints in one process through the encoded frames and prints a readable
  transcript. It is a local protocol diagnostic, not a peer session.

## Tool matrix

| Surface | Covered | Tests |
|---|---|---:|
| parser | `user[@host]`, terminal, `-l`, usage and bounds | 5 lib |
| wire | all frames, escaping, UTF-8, malformed and oversized input | 2 lib |
| session | invite/accept/decline, both-way forwarding, alert, refresh, EOF/BYE, out-of-order events | 7 lib |
| UI | invitation text, split panes, refresh, sanitisation | 3 lib |
| loopback | full encoded two-endpoint transcript | 1 lib + 5 host |
| CLI | usage, network rejection, missing registry, missing listener transport | 5 host |

The host suite is 20 tests (15 library + 5 command-line).

## Exact unsupported cases

| Case | Why |
|---|---|
| `who`-style logged-in terminal lookup | no `who`/utmp terminal registry exists; `ttyd` stores only a session's foreground group, session id and hangup bit |
| peer terminal addressing | `consd` serves one console (`tty:console`) and `ptyd` serves one anonymous master/slave pair; there is no per-terminal name or second terminal to attach |
| `talkd`/network `user@host` | not implemented; non-`localhost` hosts are rejected before any session is attempted |
| `mesg n` permission | no `mesg` utility or terminal-message flag is available |
| extended `stty iexten` controls | only printable ASCII, space, alert, refresh, EOF and interrupt are handled |
| local two-user tty-to-tty | impossible without the peer registry and a second terminal; the tested local loopback is explicitly not claimed as that surface |

## Acceptance

```
sh tests/posix/utility/run-talk.sh
cargo nextest run -p talk
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

The runner reports `1 pass, 0 fail` with 7 cases. `docs/posix/evidence/P4-talk.toml` intentionally
emits no row; the gate keeps `util.talk` missing/partial instead of claiming a false pass. No board
claim.
