<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release evidence: TCP section (K295)

Status: **ASSEMBLED; CURRENT-MASTER TARGET REVALIDATION BLOCKED.** The merged port entries, the
current-master audit/unit/checker/Linux results and the committed K264 matrix packet are OBSERVED.
The current-master target matrix is **BLOCKED**: both the 240 s `--all` acceptance and a
proportional 600 s `--matrix` attempt expired during the target initfs build under host
contention, with no target `matrix.json` produced. The committed K264 packet remains the durable
target wire evidence. This is not a release sign-off.

Branch `otto/K295`, merge `e033b23044189d2b8b3d475a80bcf121494c76bc` of current master
`eee05a0788f8f04c906440b5ec5dada991f4e5d5`.

Tag: **every row below is OBSERVED.** Each row names the artifact that supports it; the SHA-256
register is `tests/evidence/tcp/release/manifest.json`. Work that is not complete is in **Open
gaps**, not asserted as a release claim. There are no PROPOSED rows left in the manifest.

## Ported files

The eight landed FreeBSD/DragonFly entries are OBSERVED in `PROVENANCE.toml` (K268, K270 x2,
K271 x2, K272, K276, K313). K279 changes only the netd readiness test and is covered by K272's
`readiness.rs` entry. The exact per-file donor paths, revisions and licences are in the K264
evidence register.

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
| Quarantined ports: 17 claims (K254-K261, K265-K267, K269, K273-K275, K277, K278), no entries | OBSERVED | `tests/evidence/tcp/port-claims.json` |

No NetBSD TCP entry exists; the NetBSD TCP files are NRL 4-clause NO-COPY and stay quarantined.

## Matrix

The committed K264 matrix packet (`tests/evidence/tcp/run-20261121T003615Z/`, commit
`c3a2d951`) is OBSERVED: 10/10 ran, 9 pass, 1 finding. The current-master Linux side was
revalidated on the K295 merge (5/5 pass). The current-master target side is BLOCKED.

| Target/scenario | Tag | Result | Artifact |
|---|---|---|---|
| K264 Linux handshake, zero-window, window-update, retransmit, fin-rst | OBSERVED | 5/5 pass | `run-20261121T003615Z/pcaps/linux-*.pcap`; `sha256sums.txt` |
| K264 Raven zero-window, window-update, retransmit, fin-rst | OBSERVED | 4/4 pass | `run-20261121T003615Z/serial/raven-*.log` |
| K264 Raven handshake | OBSERVED | **finding:** `SYN-ACK mss=1460 ws=1 sack=True; plain-peer ws=1 win=65535` (Linux: `ws=absent win=64240`) | `run-20261121T003615Z/pcaps/raven-handshake.pcap`; `serial/raven-handshake.log` |
| K295 current-master Linux revalidation | OBSERVED | 5/5 pass on merge `e033b230` | `release/k295/logs/matrix-linux-*.log` |
| K295 current-master target revalidation | OBSERVED | **BLOCKED:** `--all` exit 124 at 240 s and `--matrix` exit 124 at 600 s during the target initfs build; no target `matrix.json`; host load 127.51 with 11 concurrent minion check-fast runs | `release/k295/target-revalidation.txt`; `run-20261121T032607Z/`; `run-20261121T033039Z/` |

The handshake finding is a TCP port gate: Raven offers a window-scale option to a peer that did
not offer one. The target build measured 431.881 s initfs + 15.668 s image in the K264 packet; the
current-master rerun did not reach packing within either cap because the host was saturated.

## Gates

| Gate | Tag | Result | Artifact |
|---|---|---|---|
| K295 current-master audit | OBSERVED | 127 files, 9 imports (7 FreeBSD, 1 DragonFly, 1 smoltcp), 27 claims, 17 quarantined, 13 donor files, 0 violations | `release/k295/audit.json`; `audit.txt` |
| K295 current-master unit | OBSERVED | **1156/1156 passed**, 0 skipped | `release/k295/unit.json`; `logs/unit.log` |
| K295 current-master checker | OBSERVED | 2 negative fixtures, 1 positive, 2 mutations caught | `release/k295/checker.json`; `logs/checker.log` |
| K295 current-master Linux matrix | OBSERVED | 5/5 pass | `release/k295/logs/matrix-linux-*.log` |
| K264 matrix | OBSERVED | 10/10 ran, 9 pass, 1 handshake finding | `run-20261121T003615Z/matrix.json` |
| K264 provenance | OBSERVED | 10565 files, 0 violations (K264 packet) | `docs/tcp-evidence.md`; K264 report |
| K295 `make check-fast` (merge `e033b230`) | OBSERVED | **`check-fast: PASSED`**; guard 40.27 s, fmt 60.72 s, profiles 0.53 s; clippy/tests skipped (no changed host-testable packages) | K295 report; `/tmp/opencode/k295-rework-checkfast.log` |
| K295 `tcp-evidence.sh --all` | OBSERVED | **TIMEOUT** at 240 s (target initfs build); audit/unit/checker/Linux completed | `release/k295/target-revalidation.txt`; `run-20261121T032607Z/` |
| K295 target matrix warm run | OBSERVED | **TIMEOUT** at 600 s (target initfs build); Linux completed, no target matrix | `release/k295/target-revalidation.txt`; `run-20261121T033039Z/` |

## Evidence packet

* `tests/evidence/tcp/release/manifest.json` - one OBSERVED row per claim with artifact and SHA-256.
* `tests/evidence/tcp/release/check.py` - validates every tag, artifact path and SHA-256;
  `--selftest` proves a missing tag/artifact/hash row is rejected.
* `tests/evidence/tcp/release/k295/` - current-master audit/unit/checker/Linux artifacts and the
  target-revalidation blocker record.
* `tests/evidence/tcp/release/k295/run-manifest.json` - host commit, revisions and input hash of
  the current-master `--all` packet.
* `tests/evidence/tcp/run-20261121T003615Z/` - the committed K264 matrix packet (target
  pcaps/serials, build logs).
* `tests/evidence/tcp/run-20261121T004851Z/` - the K264 clean audit/unit/checker packet.

Run:

```sh
timeout 30 python3 tests/evidence/tcp/release/check.py --selftest
timeout 30 python3 tests/evidence/tcp/release/check.py --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets
```

The K295 report carries the acceptance commands, exit codes, the host-contention blocker and the
open gaps. The packet is not a release sign-off until the current-master target matrix runs to
completion on a quiet host and the handshake finding is fixed or accepted as a blocker.

## Open gaps

1. **Current-master target matrix (OBSERVED BLOCKED):** the 240 s `--all` and 600 s `--matrix`
   attempts both expired during the target initfs build under host load 127.51; no target
   `matrix.json`/pcaps/serials exist for `e033b230`. A quiet-host rerun (or a lead-approved
   measured cap) is required. The K264 packet remains the durable target wire evidence.
2. **Raven handshake window scale (OBSERVED finding):** Raven advertises `ws=1` to a plain peer;
   a TCP port ticket must fix it or record it as a release blocker.
3. **Quarantined ports (OBSERVED):** K254-K261, K265-K267, K269, K273-K275, K277 and K278 have no
   provenance entries until reworked and resubmitted.
4. **K271 naming (OBSERVED):** the report says `solisten_proto`; the pinned FreeBSD tree has
   `solisten_clone`/`solisten_enqueue`. Reviewer confirmation remains.
5. **Scope (OBSERVED):** K295 is not in the reviewer-owned `tickets/evidence/scope.txt`, so
   `make evidence` has no K295 registry row yet.
6. No 24 h, live SSH/tap or Pi 4 claim is made.
