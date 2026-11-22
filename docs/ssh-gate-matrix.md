<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, one boot on the K194 tap against
`build/qtest/ssh-login.img` (master `b8472426`, K427 + K429; K430 not merged).
The chain each row asserts is in [`docs/ssh-gate-chain.md`](ssh-gate-chain.md); the row-to-chain
mapping is the lead's `tickets/ssh/SSH-CHAIN.md`. **A row is pass or fail with bytes; no class
labels.** A red row is "chain step X violated, test Y".

Run: `timeout 900 sh tests/ssh/matrix.sh` (exit 1, 6 red rows) →

| Step | Test | Expectation (source) | Result | Exact assertion / artifact |
|---|---|---|---|---|
| S0 | guest TCP accept/write over tap | BSD `accept`/`sosend` (C1) | REFERENCE | K408 accepted-socket tap fixture passes (`tests/fixtures/socket-write-exec`); a guest echo fixture would need `tests/fixtures/**` outside this ticket's Owns |
| S1 | sshd banner | OpenSSH `version.c` (C1/C2) | **FAIL** | the first peer probe read 0 bytes (`s1-peer.log`); **chain step C1/C2 violated, test S1** |
| S2 | KEXINIT packet | RFC 4251 §6 (C2/C3) | PASS | `record #1 total=960 packet_length=956 padding=9 payload=946 type=20 (SSH_MSG_KEXINIT)` |
| S3 | auth | OpenSSH `auth-passwd` (C5 precondition) | **FAIL** | the sampled correct-password session reached the prompt but no `Authenticated to`; the wrong-password probe never reached the prompt; **chain step C5 precondition violated, test S3** |
| S4 | post-auth read | POSIX `read`/FreeBSD `soreceive` (C2/C7) | **BLOCKED** | not reachable while S3 is red (no post-auth read) |
| S5 | post-auth write delivery | POSIX `write`/FreeBSD `sosend` (C5/C6/C8) | PASS | client received `channel 0: open confirm` (the 44-byte confirmation) |
| S6 | channel confirmation | RFC 4254 §5.1, OpenSSH `channels.c` (C5) | PASS | client accepted the confirmation (type 91 delivery path) |
| S7 | `ssh raven@... ls` | gate (C1-C10) | **BLOCKED + FAIL** | exact `/tmp/ssh-marker` staging needs `profiles/release.d`, outside this ticket's Owns; `ssh ... ls` did not authenticate in this run; **chain C1-C10 violated, test S7** |
| S8 | PTY | POSIX `openpty`, OpenSSH `pty.c` (C9) | **FAIL** | client: `PTY allocation request failed on channel 0`; **chain step C9 violated, test S8 (K430)** |

Artifacts (SHA-256):

```text
48db40c52ce93cf7d24715549fa8b7f1819d4c9322c623af651f4d5a84655c43  build/ssh/matrix/matrix.tsv
83f8b48b7d724105c281bdc45dffdfd1808b6eccaa73c6728980bda9b0c11ca7  build/ssh/matrix/serial.log
a31f869d4fd9af990145e27df1d543a118e559b1fb1de5b92adb2ea846f34cb4  matrix run log
32159c4036ec3c4db0f8e5eabefa26876270bab50306d2cc4c7315f1a9a26fec  make check-fast log
```

## Pre-fix / mutation proof

The rows were written and first run on current master; the earlier K195 rounds provide the
pre-fix failures for the fixed steps: S5 failed on the pre-K409 tree (confirmation never reached
the client) and passes now; S2 failed on the pre-K372 KEXINIT path. The still-red rows are the
current open defects (C1/C2 intermittency, C5 auth reachability, C9/K430), each with its own
failing test above. No row is asserted from a trace or a hypothesis.

## Notes

- The matrix deliberately uses one boot and one attempt per row; an intermittent step is reported
  red with its exact bytes, never retried to green.
- S0's exact echo payload needs a guest echo fixture and S7's marker needs a profile fragment;
  both are outside K434's Owns (`tests/ssh/**`, `tests/fixtures/ssh-peer/**`,
  `docs/ssh-gate-matrix.md`) and are reported BLOCKED, not guessed.
