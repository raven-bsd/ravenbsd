<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release evidence: TCP section (K295)

Status: **ASSEMBLED; TARGET REVALIDATION OPEN.** The merged port entries and the committed K264
matrix packet are OBSERVED. The K295 `--all` run completed the audit, unit, checker and Linux
matrix, then reached the ticket's 240 s cap during the target initfs build; the current-master
target matrix is therefore PROPOSED for a measured-cap rerun. This is not a release sign-off.

Tags: **OBSERVED** means the artifact exists in this tree and was measured; **PROPOSED** means the
work or rerun is not yet complete. Every row names the artifact that supports it.

## Ported files

The eight landed FreeBSD/DragonFly entries are OBSERVED in `PROVENANCE.toml` (K268, K270 x2,
K271 x2, K272, K276, K313). K279 changes only the netd readiness test and is covered by K272's
`readiness.rs` entry. The exact per-file donor paths, revisions and licences are in the K264
evidence register (`tests/evidence/tcp/port-claims.json`, `docs/tcp-evidence.md`).

| Claim | Tag | Artifact |
|---|---|---|
| K268: DragonFly `sys/netinet/tcp_sack.c` -> `src/sack.rs` (BSD-3-Clause) | OBSERVED | `PROVENANCE.toml`; `crates/raven-tcpip/src/sack.rs` |
| K270: FreeBSD `tcp_ecn.c`/`tcp_ecn.h`/`tcp.h` -> `src/ecn.rs` (BSD-3-Clause) | OBSERVED | `PROVENANCE.toml`; `crates/raven-tcpip/src/ecn.rs` |
| K270: FreeBSD `tcp_input.c`/`tcp_subr.c`/`ip_icmp.c` -> `src/pmtu.rs` (BSD-3-Clause) | OBSERVED | `PROVENANCE.toml`; `crates/raven-tcpip/src/pmtu.rs` |
| K271: FreeBSD `tcp_usrreq.c`/`tcp_subr.c`/`uipc_socket.c` -> `src/socket.rs` (BSD-3-Clause) | OBSERVED | `PROVENANCE.toml`; `crates/raven-tcpip/src/socket.rs` |
| K271: FreeBSD `uipc_socket.c` -> `src/listen.rs` (BSD-3-Clause) | OBSERVED | `PROVENANCE.toml`; `crates/raven-tcpip/src/listen.rs` |
| K272: FreeBSD `sys/sys/socketvar.h` -> `src/readiness.rs` (BSD-3-Clause) | OBSERVED | `PROVENANCE.toml`; `crates/raven-tcpip/src/readiness.rs` |
| K276: FreeBSD `tcp_syncache.c`/`.h`/`tcp_timer.c` -> `src/syncache.rs` (BSD-3-Clause) | OBSERVED | `PROVENANCE.toml`; `crates/raven-tcpip/src/syncache.rs` |
| K313: FreeBSD `tcp_output.c`/`tcp_timer.c` -> `src/tcp_data.rs` (BSD-3-Clause) | OBSERVED | `PROVENANCE.toml`; `crates/raven-tcpip/src/tcp_data.rs` |
| K279: netd readiness test, covered by K272 | OBSERVED | `userland/servers/netd/src/netd_ut.rs`; K272 entry |
| Quarantined ports (17 claims: K254-K261, K265-K267, K269, K273-K275, K277, K278) | PROPOSED | `tests/evidence/tcp/port-claims.json` |

No NetBSD TCP entry exists; the NetBSD TCP files are NRL 4-clause NO-COPY and stay quarantined.

## Matrix

The committed K264 matrix packet (`tests/evidence/tcp/run-20261121T003615Z/`, commit
`c3a2d951`) is OBSERVED: 10/10 ran, 9 pass, 1 finding.

| Target/scenario | Result | Artifact |
|---|---|---|
| Linux handshake, zero-window, window-update, retransmit, fin-rst | 5/5 pass | `run-20261121T003615Z/pcaps/linux-*.pcap`; `sha256sums.txt` |
| Raven zero-window, window-update, retransmit, fin-rst | 4/4 pass | `run-20261121T003615Z/serial/raven-*.log` |
| Raven handshake | **OBSERVED finding:** `SYN-ACK mss=1460 ws=1 sack=True; plain-peer ws=1 win=65535` (Linux: `ws=absent win=64240`) | `run-20261121T003615Z/pcaps/raven-handshake.pcap`; `serial/raven-handshake.log` |
| K295 Linux revalidation | 5/5 pass (OBSERVED) | `tests/evidence/tcp/release/k295/logs/matrix-linux-*.log` |
| K295 target revalidation | **PROPOSED:** `--all` TIMEOUT at 240 s during the target initfs build; the K264 packet measured 431.881 s initfs + 15.668 s image | `release/k295/run-manifest.json`; `run-20261121T003615Z/logs/build-initfs.log` |

The handshake finding is a TCP port gate: Raven offers a window-scale option to a peer that did
not offer one. The current-master target matrix needs a measured cap above the 431.881 s build;
the ticket's 240 s cap is insufficient.

## Gates

| Gate | Tag | Result | Artifact |
|---|---|---|---|
| K295 audit | OBSERVED | 9 imports (7 FreeBSD, 1 DragonFly, 1 smoltcp), 27 claims, 17 quarantined, 13 donor files, 0 violations | `release/k295/audit.json` |
| K295 unit | OBSERVED | 930/930 passed, 0 skipped | `release/k295/unit.json` |
| K295 checker | OBSERVED | 2 negative fixtures, 1 positive, 2 mutations caught | `release/k295/checker.json` |
| K264 matrix | OBSERVED | 10/10 ran, 9 pass, 1 handshake finding | `run-20261121T003615Z/matrix.json` |
| K264 provenance | OBSERVED | 10565 files, 0 violations | `docs/tcp-evidence.md`; K264 report |
| K264 check-fast | OBSERVED | `check-fast: PASSED` at the K264 merge | K264 report `tickets/otto/reports/K264.md` |
| K295 `tcp-evidence.sh --all` | PROPOSED | **TIMEOUT** at 240 s (target initfs build); audit/unit/checker/Linux completed | `release/k295/run-manifest.json`; K295 report |
| K295 `make check-fast` | OBSERVED in the K295 report | run at the release packet commit | K295 report |

## Evidence packet

* `tests/evidence/tcp/release/manifest.json` - machine-readable claim/tag/artifact/hash rows.
* `tests/evidence/tcp/release/check.py` - validates every claim tag, artifact path and SHA-256;
  `--selftest` proves a missing tag/artifact/hash row is rejected.
* `tests/evidence/tcp/release/k295/` - the current audit/unit/checker/Linux artifacts.
* `tests/evidence/tcp/run-20261121T003615Z/` - the committed K264 matrix packet (pcaps/serials).
* `tests/evidence/tcp/run-20261121T004851Z/` - the K264 clean audit/unit/checker packet.

Run:

```sh
timeout 30 python3 tests/evidence/tcp/release/check.py --selftest
timeout 30 python3 tests/evidence/tcp/release/check.py
```

The K295 report carries the acceptance commands and exit codes; the release gate is not signed
until the target matrix is rerun with a measured cap and the handshake finding is resolved or
accepted as a blocker.

## Open gaps

1. **Target matrix revalidation (PROPOSED):** rerun `scripts/tcp-evidence.sh --matrix` on the
   current master with a cap measured from the 431.881 s initfs build; the 240 s `--all` cap is
   insufficient. The K264 packet remains the committed wire evidence.
2. **Handshake window scale (OBSERVED finding):** Raven advertises `ws=1` to a plain peer; a TCP
   port ticket must fix or record the release blocker.
3. **Quarantined ports (PROPOSED):** K254-K261, K265-K267, K269, K273-K275, K277, K278 have no
   entries until reworked and resubmitted.
4. **K271 naming:** the K271 report notes `solisten_proto`; the pinned FreeBSD tree has
   `solisten_clone`/`solisten_enqueue`; reviewer confirmation remains.
5. No 24 h, live SSH/tap or Pi 4 claim is made here.
