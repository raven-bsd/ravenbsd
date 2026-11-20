<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP evidence packets (K264)

This directory is owned by K264. It holds the machine-readable port-claims intake and one
timestamped evidence packet per run of `scripts/tcp-evidence.sh`.

## Files

* `port-claims.json` - the independent intake for the K254-K263 TCP wave. Each entry records the
  ticket, lane, report, kind (`port` or `test-only`), current status and the exact NetBSD donor
  files. A `port` claim is `pending` until its code lands; it becomes `landed` only together with
  a matching `[[import]]` in `PROVENANCE.toml`.
* `run-<UTC>/manifest.json` - commit, dirty state, host tools and the SHA-256 of every input.
* `run-<UTC>/audit.txt` / `audit.json` - the provenance and claims audit.
* `run-<UTC>/unit.json` and `logs/unit.log` - `cargo nextest run -p raven-tcpip` counts and output.
* `run-<UTC>/matrix.json`, `matrix-blocked.txt`, `logs/matrix-*`, `pcaps/`, `serial/` - the K253
  conformance matrix when the harness is in the tree.
* `run-<UTC>/summary.json`, `summary.txt`, `command.log`, `sha256sums.txt` - the packet result,
  commands and integrity hashes.

## Regenerate

```sh
timeout 120 sh scripts/tcp-evidence.sh --selftest
timeout 240 sh scripts/tcp-evidence.sh --unit
timeout 300 sh scripts/tcp-evidence.sh --audit \
    --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets
timeout 900 sh scripts/tcp-evidence.sh --matrix
```

`--all` runs the audit, the unit suite and the matrix in one packet. The matrix is recorded as
`blocked` when `scripts/net-conformance.sh` is absent (K253 not merged) or the host cannot create
the tap namespace; it is never recorded as a pass.

## Intake rule

A port report marked `DONE` is accepted only when its donor files pass the licence audit:

* Three-clause donors (WIDE/NetBSD/UCB) may be ported with a `BSD-3-Clause` entry.
* Two-clause-only donors use a `BSD-2-Clause` entry.
* Four-clause donors (the NRL/UCB advertising clause) are **NO-COPY**. Those files must be a
  clean-room implementation with `copy: "clean-room"` and **no** `[[import]]`; the claim status is
  `clean-room`, not `landed`.

For a permitted donor, the exact block is:

```toml
[[import]]
path = "crates/raven-tcpip/src/<file>.rs"
upstream = "netbsd"
upstream_path = "sys/netinet/<donor>.c"
upstream_rev = "36eddebcc36cb0775aa2962005b9c05419a6ff01"
license = "BSD-3-Clause"
tier = "A"
copied = "YYYY-MM-DD"
modified = true
notes = "<functions and deviations>"
```

The integrator appends the block to `PROVENANCE.toml` and changes the claim status to `landed` in
the same commit. The audit fails if a pending claim gains an entry, if a landed claim lacks one, if
a clean-room claim gains one, if a report is DONE while the claim is pending, or if a file says it
was ported from NetBSD without an entry. That is what makes the packet an independent audit rather
than a summary of the reports.
