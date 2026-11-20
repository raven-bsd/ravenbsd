<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP provenance and evidence (K264)

Status: **PARTIAL (rework round 1).** The NetBSD TCP files are NRL 4-clause **NO-COPY**. No
NetBSD TCP provenance entry exists or may be added. The permitted donors are **FreeBSD
`sys/netinet`** (BSD-3-Clause, no advertising; `cc/cc_newreno.c` is BSD-2-Clause) and
**DragonFly `sys/netinet/tcp_sack.c`** (BSD-3-Clause). Every NetBSD-derived port branch/report is
quarantined until it is reworked against those donors. The port reports currently contain no
FreeBSD/DragonFly entry, so this packet adds no `PROVENANCE.toml` entry. The K253 packet matrix is
still blocked because the harness is not merged into `origin/master`.

## Commands and results

All commands are foreground with an explicit cap.

| Command | Cap | Exit | Result |
|---|---|---|---|
| `sh scripts/tcp-evidence.sh --selftest` | 60 s | 0 | `PASS netbsd_imports=0 quarantined=8 claims=11 matrix_blocker=detected` |
| `cargo nextest run -p raven-tcpip` | 180 s | 0 | **647 tests run: 647 passed, 0 skipped** |
| `sh scripts/tcp-evidence.sh --audit --reports-root <tickets> --freebsd-root <fb> --dragonfly-root <df>` | 120 s | 0 | **pass**: 67 files, 0 NetBSD imports, 8 quarantined claims, 15 donor files verified, 2 rejected entries, 0 violations |
| `sh scripts/tcp-evidence.sh --all ...` | 240 s | 2 | audit pass, unit pass, matrix blocked |
| `make check-fast` | 420 s | 0 | **`check-fast: PASSED`** |

## Why NetBSD TCP is quarantined

The pinned NetBSD tree (`36eddebcc36cb0775aa2962005b9c05419a6ff01`) has a second copyright block
in the key TCP files:

| NetBSD donor | Header | Decision |
|---|---|---|
| `sys/netinet/tcp_output.c` | WIDE 3-clause **plus NRL 4-clause advertising** | **NO-COPY** |
| `sys/netinet/tcp_input.c` | WIDE 3-clause **plus NRL 4-clause advertising** | **NO-COPY** |
| `sys/netinet/tcp_congctl.c` | NetBSD/WIDE **plus NRL 4-clause advertising** | **NO-COPY** |
| `sys/netinet/tcp_sack.c` | NetBSD/UCB **plus NRL 4-clause advertising** | **NO-COPY** |
| `sys/netinet/tcp_var.h` | NRL 4-clause advertising | **NO-COPY** |

A four-clause file is NO-COPY under the project donor rules. The audit therefore rejects any
`upstream = "netbsd"` import under `crates/raven-tcpip`, and the intake marks every NetBSD-derived
branch/report quarantined:

```text
K254 jerry/K254   K255 carl/K255    K256 mark/K256    K257 stuart/K257
K258 dave/K258    K259 mel/K259     K260 kevin/K260   K261 tim/K261
```

K258's clean-room implementation and K259's two proposed NetBSD entries are held; K259's ecn.rs
and pmtu.rs entries are **rejected** because they cite the NRL files. K260's report is DONE but
still NetBSD-derived, so it is quarantined rather than accepted with a missing entry.

## Permitted donors

The audit reads the pinned donor revisions from the checkout's git objects, so the sparse
worktrees do not hide the files:

| Donor | Revision | Licence | Files checked |
|---|---|---|---|
| FreeBSD | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-3-Clause (cc_newreno BSD-2-Clause) | `sys/netinet/tcp_output.c`, `tcp_input.c`, `tcp_timer.c`, `tcp_subr.c`, `tcp_usrreq.c`, `cc/cc_newreno.c`; `sys/sys/socketvar.h`; `sys/kern/sys_socket.c`, `uipc_socket.c` |
| DragonFly | `94448f622a06cde90ba42d688e9cab2448ad9ab3` | BSD-3-Clause | `sys/netinet/tcp_sack.c` |

No advertising clause is present in either donor's checked files. The donor HEADs match the
pins; the audit records `source=git` for the sparse files.

## Provenance entries

`PROVENANCE.toml` still has **no** `crates/raven-tcpip` NetBSD import. The only entry under the
crate is the pre-existing smoltcp DHCP client (`src/dhcp.rs`, 0BSD, tier A, revision
`8b0e5c85...`), unrelated to TCP.

No FreeBSD/DragonFly entry is added yet: the current port reports and branches still map NetBSD
`tcp_input.c`/`tcp_output.c`/`tcp_sack.c`, and adding a FreeBSD/DragonFly entry for that code would
be a false provenance claim. A reworked port report must send the exact entry; the audit will add
it only with `status=landed` and the matching code in the same commit.

## Port-claim intake

A reworked report must send one entry per product file:

```toml
[[import]]
path = "crates/raven-tcpip/src/<file>.rs"
upstream = "freebsd"          # or "dragonfly" for tcp_sack
upstream_path = "sys/netinet/<donor>.c"
upstream_rev = "133344405f532bd54ff3629a77d4c4e062f01fdb"
license = "BSD-3-Clause"      # BSD-2-Clause for cc/cc_newreno.c
tier = "A"
copied = "YYYY-MM-DD"
modified = true
notes = "<donor functions and every divergence>"
```

Then flip the claim from `quarantined` to `landed` in the same commit. The audit fails if a
NetBSD import appears, if a quarantined claim gains an import, if a landed claim lacks one, or if
the donor symbol/licence does not match the pinned donor.

## Evidence packet

A run writes `tests/evidence/tcp/run-<UTC>/`:

```text
manifest.json          commit, dirty state, host tools, input and donor revisions
audit.txt/audit.json   NetBSD quarantine, FreeBSD/DragonFly donors, claims, violations
unit.json              nextest counts and exit
logs/unit.log          full nextest output
matrix.json            per-target/per-scenario status (blocked when K253 is absent)
matrix-blocked.txt     the exact missing path and owner
summary.json/summary.txt  overall result and exit
command.log            phase and command index
sha256sums.txt         SHA-256 of every packet file
```

The packet is immutable evidence: `sha256sums.txt` covers it, and `manifest.json` records the
commit and donor pins. Editing a packet after the run invalidates its hashes.

## Reproduce

```sh
timeout 60  sh scripts/tcp-evidence.sh --selftest
timeout 180 cargo nextest run -p raven-tcpip
timeout 120 sh scripts/tcp-evidence.sh --audit \
    --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets \
    --freebsd-root /home/bo/work/ravenos-workspace/ravenbsd-resources/freebsd \
    --dragonfly-root /home/bo/work/ravenos-workspace/ravenbsd-resources/dragonfly
timeout 240 sh scripts/tcp-evidence.sh --all \
    --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets \
    --freebsd-root /home/bo/work/ravenos-workspace/ravenbsd-resources/freebsd \
    --dragonfly-root /home/bo/work/ravenos-workspace/ravenbsd-resources/dragonfly
```

The matrix becomes available when `scripts/net-conformance.sh` lands (K253, branch `bob/K253`
`ef5c1158`, not merged). Measure its runtime then and use that measurement plus a margin for the
cap.

## Checker regression tests

`python3 scripts/tcp-evidence_ut.py` runs three fixtures: a landed port with its required entry
deleted, a NetBSD/NRL import, and a valid FreeBSD import. The checker must reject the first two and
accept the third. The test then mutates a temporary copy of the checker to remove the NetBSD rule
and the missing-entry rule; the bad fixtures must pass under those mutants, proving the test catches
the regression. The output is stored as the `checker` phase in the evidence packet.

## Limits

* No 24 h or live SSH/tap claim is made here; live SSH is the Bob integration gate.
* No product, kernel, libc or network code is changed by K264.
* The donor audit reads the pinned donor objects and checks the licence class and symbol presence,
  not copied-expression similarity; the literal-port reviewers remain responsible for the
  file-by-file review.
* The packet is partial until the NetBSD-derived ports are reworked against FreeBSD/DragonFly and
  K253 lands; the tool records that state instead of hiding it.
