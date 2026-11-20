<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP provenance and evidence (K264)

Status: **PARTIAL.** The audit, the unit evidence and the reproducible packet are implemented and
run. The audit intentionally **fails** on one upstream port report (K260) and rejects two proposed
entries (K259) because the donor files carry the NRL 4-clause advertising clause. The packet-level
conformance matrix is blocked because the K253 harness is not on `origin/master`.

The K264 tool itself is green: its self-test passes, the unit suite is 647/647, and the audit
correctly rejects unsupported provenance instead of recording it. The tree is not yet
provenance-clean until the upstream items below are fixed.

## Commands and results

All commands are foreground with an explicit cap.

| Command | Cap | Exit | Result |
|---|---|---|---|
| `sh scripts/tcp-evidence.sh --selftest` | 60 s | 0 | `PASS netbsd_imports=0 other_imports=1 claims=11 matrix_blocker=detected` |
| `cargo nextest run -p raven-tcpip` | 180 s | 0 | **647 tests run: 647 passed, 0 skipped** (1.186 s) |
| `sh scripts/tcp-evidence.sh --audit --reports-root <tickets> --donor-root <netbsd>` | 120 s | 1 | **fail**: 67 files, 0 NetBSD imports, 1 smoltcp/0BSD, 11 claims, 5 clean-room claims, 2 rejected entries, **1 violation** |
| `sh scripts/tcp-evidence.sh --all ...` | 240 s | 1 | audit fail, unit pass, matrix blocked |
| `make check-fast` | 420 s | 0 | **`check-fast: PASSED`** |

The audit violation is:

```text
K260: DONE permitted port but status is pending; add the exact entry and set status=landed
```

K260's pushed branch (`kevin/K260`, `61073d22`) changes `socket.rs`, `listen.rs` and
`tests/listen.rs` but adds no `PROVENANCE.toml` entry, although its donors (`tcp_usrreq.c`,
`tcp_subr.c`) are three-clause and therefore copyable. The audit refuses to invent the entry; K260
must send it or restate the work as clean-room.

## Donor licence audit

The audit reads the pinned NetBSD checkout (`36eddebcc36cb0775aa2962005b9c05419a6ff01`) and
classifies each donor header. It does not trust the report prose.

| Donor file | Header finding | Decision |
|---|---|---|
| `sys/netinet/tcp_output.c` | WIDE 3-clause **plus NRL 4-clause advertising** | **NO-COPY**; clean-room only |
| `sys/netinet/tcp_input.c` | WIDE 3-clause **plus NRL 4-clause advertising** | **NO-COPY**; clean-room only |
| `sys/netinet/tcp_congctl.c` | NetBSD/WIDE **plus NRL 4-clause advertising** | **NO-COPY**; clean-room only |
| `sys/netinet/tcp_sack.c` | NetBSD/UCB **plus NRL 4-clause advertising** | **NO-COPY**; clean-room only |
| `sys/netinet/tcp_var.h` | NRL 4-clause advertising | **NO-COPY**; clean-room only |
| `sys/netinet/tcp_subr.c` | WIDE/NetBSD/UCB 3-clause, no NRL | BSD-3-Clause, copy permitted |
| `sys/netinet/tcp_timer.c` | WIDE/NetBSD/UCB 3-clause, no NRL | BSD-3-Clause, copy permitted |
| `sys/netinet/tcp_usrreq.c` | WIDE/NetBSD/UCB 3-clause, no NRL | BSD-3-Clause, copy permitted |
| `sys/sys/socketvar.h` | NetBSD 2-clause + UCB 3-clause | BSD-3-Clause, copy permitted |
| `sys/kern/uipc_socket.c` | NetBSD 2-clause + UCB 3-clause | BSD-3-Clause, copy permitted |
| `sys/kern/uipc_socket2.c` | NetBSD 2-clause + UCB 3-clause | BSD-3-Clause, copy permitted |

Consequences for the wave:

* **K258** (`cc.rs`) correctly records no entry: `tcp_congctl.c`/`tcp_input.c`/`tcp_output.c`
  are NO-COPY and the report is a clean-room implementation. The claim is `clean-room`.
* **K259** proposed two entries (`ecn.rs`, `pmtu.rs`), but both cite `tcp_input.c`/`tcp_output.c`
  and/or `tcp_var.h`, all NRL 4-clause. Both entries are **rejected**; the claim is `clean-room`.
  `ecn.rs`/`pmtu.rs` must stay clean-room with no `[[import]]`.
* **K254** (`tcp_output.c`/`tcp_input.c`) and **K255** (`tcp_input.c`) name NO-COPY donors. A
  literal port is not allowed; their output/input code must be clean-room, with no entry.
* **K257** (`tcp_sack.c`) is NO-COPY; `sack.rs` must be clean-room. The ticket's
  `IsLost`/`NextSeg`/`Pipe` names are FreeBSD/RFC 6675 names, not NetBSD symbols, so they cannot
  be presented as a NetBSD port either.
* **K256** may copy only `tcp_timer.c`; `recovery.rs` must not copy `tcp_input.c` (NO-COPY).
* **K261** may copy `socketvar.h`/`uipc_socket.c` (BSD-3-Clause when both notices are retained);
  it is still `WORKING`, so no entry exists yet.
* **K260** is the only DONE permitted port without an entry: the audit violation above.

The standard checker (`cargo run -q -p raven-provenance -- check --root .`) passes because there is
no false entry in `PROVENANCE.toml`. The K264 audit is stricter: it fails when a DONE report's
entry is missing, when a clean-room claim has an entry, or when an entry's licence does not match
the donor header.

## Why there are no TCP imports yet

`PROVENANCE.toml` still has no `crates/raven-tcpip` NetBSD import. The only entry under the crate
is the pre-existing smoltcp DHCP client (`src/dhcp.rs`, 0BSD, tier A, revision `8b0e5c85...`),
unrelated to this wave.

The reasons are evidential, not an omission:

1. K260's entry is missing (the port is DONE but the report has no exact block).
2. K259's entries are rejected by the NRL 4-clause donor.
3. K254/K255/K257/K258/K259 are clean-room claims and must not carry an import.
4. K256/K261 are still `WORKING`; their entries will be collected when they land.

Adding any of those entries now would be exactly the unsupported claim this ticket must reject.

## Conformance matrix

`scripts/tcp-evidence.sh --matrix` runs `scripts/net-conformance.sh` for the five K253 scenarios
(`handshake`, `zero-window`, `window-update`, `retransmit`, `fin-rst`) against the Linux baseline
and the Raven target, storing pcaps and serial logs per run. On this tree the harness is absent
(`tests/net-conformance/harness.py`, `scripts/net-conformance*` and the fixture are not on
`origin/master`), so the packet records `matrix: blocked` with that exact reason. A missing
harness, a non-root host or a tap-namespace failure is `blocked` (exit 77), never a pass.

K263 produced independent Linux baselines on `phil/K263`, but they are not the K253 harness and are
not merged; the packet does not substitute them for the named scenarios.

## Evidence packet

A run writes `tests/evidence/tcp/run-<UTC>/`:

```text
manifest.json          commit, dirty state, host tools, input SHA-256
audit.txt/audit.json   per-file SPDX, donor licences, symbols, imports, claims, violations
unit.json              nextest counts and exit
logs/unit.log          full nextest output
matrix.json            per-target/per-scenario status (blocked when K253 is absent)
matrix-blocked.txt     the exact missing path and owner
summary.json/summary.txt  overall result and exit
command.log            phase and command index
sha256sums.txt         SHA-256 of every packet file
```

The commit of record is in `manifest.json`; `sha256sums.txt` covers the packet. Editing a packet
after the run invalidates its hashes.

## Reproduce

```sh
timeout 60  sh scripts/tcp-evidence.sh --selftest
timeout 180 cargo nextest run -p raven-tcpip
timeout 120 sh scripts/tcp-evidence.sh --audit \
    --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets \
    --donor-root /home/bo/work/ravenos-workspace/ravenbsd-resources/netbsd
timeout 240 sh scripts/tcp-evidence.sh --all \
    --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets \
    --donor-root /home/bo/work/ravenos-workspace/ravenbsd-resources/netbsd
```

When K253 lands, run the matrix and measure its runtime; use that measurement plus a margin for
the cap. The five scenarios are fixed in `scripts/tcp-evidence.py`; K263 can add cases only by
updating the harness and this list.

## Limits

* No 24 h or live SSH/tap claim is made here; live SSH is the Bob integration gate.
* No product, kernel, libc or network code is changed by K264.
* The donor audit reads the first 120 lines of each pinned donor file. It verifies the licence
  class and symbol presence, not copied-expression similarity; the literal-port reviewers remain
  responsible for the file-by-file review.
* The packet is partial until K253 and the port reports are fixed; the tool records that state
  instead of hiding it.
