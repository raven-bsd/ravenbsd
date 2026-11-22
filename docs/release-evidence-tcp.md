<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release evidence: TCP section (K295)

Status: **ASSEMBLED; LEAD-AUTHORIZED 360 s `--all` PASSED; TARGET MATRIX 10/10 ON CURRENT MASTER;
RELEASE GATE NO-GO.** Branch `otto/K295` merges current master
`269d43dc9500389d32659d69adf9596d6671847e` as
`e7b21a240ae8c77ec4e8355ae03c9a7a73113b84`. The exact
`timeout 360 sh scripts/tcp-evidence.sh --all` exits 0: audit pass, unit 1262/1262, checker pass,
matrix 10/10, initfs 192.361 s, image 1.937 s, clean tree. The 360 s cap is the lead-authorized
measured cap recorded by Directive rev 87; the earlier 240 s cap was below the measured ~250 s
all-phase. This is not a release sign-off: the TCP release gate stays NO-GO until SSH login, the
24 h soak and the independent audit are present.

Tag: **every row below is OBSERVED.** Each row names the artifact that supports it; the SHA-256
register is `tests/evidence/tcp/release/manifest.json`. There are no PROPOSED rows.

## Ported files

The eight earlier landed FreeBSD/DragonFly entries are OBSERVED in `PROVENANCE.toml` (K268,
K270 x2, K271 x2, K272, K276, K313), plus K279's netd test covered by K272. K269 (FreeBSD
NewReno/CUBIC/RACK, merged `f6235c82`) is OBSERVED in the source and the K269 report, but its
`PROVENANCE.toml` entry for `cc.rs`/`recovery.rs` is K264-owned and still absent; that is an open
gap, not a completed provenance claim.

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
| K269: FreeBSD `cc_newreno.c`/`cc.c`/`cc_cubic.c` -> `src/cc.rs`; `tcp_stacks/rack.c` core -> `src/recovery.rs` (merged `f6235c82`) | OBSERVED | `crates/raven-tcpip/src/cc.rs`; `src/recovery.rs`; `dave/reports/K269.md`; **PROVENANCE entry missing (gap)** |
| K279: netd readiness test, covered by K272 | OBSERVED | `userland/servers/netd/src/netd_ut.rs`; K272 entry |
| Quarantined ports: K254-K261, K265-K267, K273-K275, K277, K278; the K264 intake also still lists K269 (stale) | OBSERVED | `tests/evidence/tcp/port-claims.json` |

RACK donor-function audit on the merge (K315 clean cells):

| Audit | Tag | Result | Artifact |
|---|---|---|---|
| RACK acceptance | OBSERVED | `donors=219 register=219 target=218 accepted=219 gaps=0 missing=0 duplicate=0`; `k315: missing=0 duplicate=0 unknown=0 empty_rust=0 empty_test=0 empty_mutation=0 not_pass=0` | `release/k295/logs/k295-r3-rack-acceptance.log` |
| RACK register | OBSERVED | `donors=219 rows=219 missing=0 unknown=0 duplicate=0 bad_owner=0 empty_rust=0 empty_test=0 empty_mutation=0 not_pass=0 missing_symbol=0 missing_test=0` | `release/k295/logs/k295-r3-rack-register.log` |

## Matrix

The current-master target matrix is the new wire evidence: 10/10 pass, including all five Raven
scenarios. The K264 packet remains the historical comparison; its `ws=1` handshake finding is no
longer reproduced (current: `ws=3` only when the peer offers, plain-peer `ws=absent`).

| Target/scenario | Tag | Result | Artifact |
|---|---|---|---|
| Current-master Linux handshake, zero-window, window-update, retransmit, fin-rst | OBSERVED | 5/5 pass | `release/k295/target/logs/matrix-linux-*.log`; pcaps in `run-20261121T053533Z` |
| Current-master Raven handshake | OBSERVED | **pass** `SYN-ACK mss=1460 ws=3 sack=True; plain-peer ws=absent win=65535` | `release/k295/target/serial/raven-handshake.log` |
| Current-master Raven zero-window, window-update, retransmit, fin-rst | OBSERVED | 4/4 pass | `release/k295/target/serial/raven-*.log` |
| Current-master matrix build | OBSERVED | initfs 192.361 s, image 1.937 s, 10/10 | `release/k295/target/matrix.json`; `logs/build-*.log` |
| K264 matrix packet (historical) | OBSERVED | 10/10 ran, 9 pass, 1 handshake finding | `run-20261121T003615Z/matrix.json`; pcap/serial |

## Gates

| Gate | Tag | Result | Artifact |
|---|---|---|---|
| Current-master audit | OBSERVED | 134 files, 9 imports (7 FreeBSD, 1 DragonFly, 1 smoltcp), 27 claims, 17 quarantined (K269 stale), 13 donor files, 0 violations | `release/k295/audit.json`; `audit.txt` |
| Current-master unit | OBSERVED | **1262/1262 passed**, 0 skipped | `release/k295/unit.json`; `logs/unit.log` |
| Current-master checker | OBSERVED | 2 negative fixtures, 1 positive, 2 mutations caught | `release/k295/checker.json`; `logs/checker.log` |
| Current-master target matrix | OBSERVED | **10/10 pass**, 0 fail, 0 blocked; initfs 192.361 s, image 1.937 s | `release/k295/target/matrix.json` |
| RACK acceptance/register | OBSERVED | 219/219/219 accepted, gaps=0, duplicate=0; K315 empty cells 0; register rows=219, missing=0 | `release/k295/logs/k295-r3-rack-*.log` |
| `timeout 360 sh scripts/tcp-evidence.sh --all` | OBSERVED | **exit 0**, 276 s; audit/unit/checker/matrix all pass | `release/k295/target-revalidation.txt`; `run-20261121T053533Z` |
| `make check-fast` | OBSERVED | run on the packet commit; quoted in the K295 report | K295 report |
| `make evidence-check` | OBSERVED | **`evidence check: PASSED`**; K295 not in the reviewer-owned scope, so no K295 registry row | K295 report |

**Release gate: NO-GO.** The matrix is present, but the gate requires the matrix, SSH login, a 24 h
soak and an independent audit together. SSH login, the 24 h soak and the independent audit are not
in this packet.

## Evidence packet

* `tests/evidence/tcp/release/manifest.json` - one OBSERVED row per claim with artifact and SHA-256.
* `tests/evidence/tcp/release/check.py` - validates every tag, artifact path and SHA-256;
  `--selftest` proves missing tag/artifact/hash rows are rejected.
* `tests/evidence/tcp/release/k295/` - current audit/unit/checker, measured build logs, RACK audit
  logs and the 360 s acceptance record.
* `tests/evidence/tcp/release/k295/target/` - current-master matrix.json, target serials, scenario
  logs, summary and the pcap/serial hash register. The pcaps themselves stay in the generated
  `run-20261121T053533Z/pcaps/` packet outside the K295 Owns globs.
* `tests/evidence/tcp/run-20261121T003615Z/` - committed K264 matrix packet.
* `tests/evidence/tcp/run-20261121T004851Z/` - K264 clean audit/unit/checker packet.

Run:

```sh
timeout 30 python3 tests/evidence/tcp/release/check.py --selftest
timeout 30 python3 tests/evidence/tcp/release/check.py --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets
```

## Open gaps

1. **K269 provenance (OBSERVED):** the code merged in `f6235c82` but `PROVENANCE.toml` has no
   `cc.rs`/`recovery.rs` entry (K264-owned, outside K295 Owns). The K264 intake still lists K269
   quarantined; the packet records the code and the missing entry separately.
2. **Release gate NO-GO (OBSERVED):** matrix is present; SSH login, 24 h soak and independent
   audit are absent.
3. **Remaining quarantined ports (OBSERVED):** K254-K261, K265-K267, K273-K275, K277, K278.
4. **K271 naming (OBSERVED):** `solisten_proto` vs the pinned FreeBSD
   `solisten_clone`/`solisten_enqueue`; reviewer confirmation remains.
5. **Scope (OBSERVED):** K295 is not in the reviewer-owned `tickets/evidence/scope.txt`.
6. No 24 h, live SSH/tap or Pi 4 claim is made.
