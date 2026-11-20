<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP evidence packets (K264)

This directory is owned by K264. It holds the machine-readable port-claims intake and one
timestamped evidence packet per run of `scripts/tcp-evidence.sh`.

## Files

* `port-claims.json` - the independent intake for the K254-K263 TCP wave. It records the permitted
  FreeBSD/DragonFly donor pins, the NetBSD TCP quarantine register and each ticket's status. A
  NetBSD-derived port is `quarantined`; a reworked port becomes `landed` only together with a
  matching FreeBSD/DragonFly `[[import]]` in `PROVENANCE.toml`.
* `run-<UTC>/manifest.json` - commit, dirty state, host tools, input SHA-256 and donor pins.
* `run-<UTC>/audit.txt` / `audit.json` - the provenance, quarantine and donor audit.
* `run-<UTC>/unit.json` and `logs/unit.log` - `cargo nextest run -p raven-tcpip` counts and output.
* `run-<UTC>/matrix.json`, `matrix-blocked.txt`, `logs/matrix-*`, `pcaps/`, `serial/` - the K253
  conformance matrix when the harness is in the tree.
* `run-<UTC>/summary.json`, `summary.txt`, `command.log`, `sha256sums.txt` - the packet result,
  commands and integrity hashes.

## Regenerate

```sh
timeout 60  sh scripts/tcp-evidence.sh --selftest
timeout 240 sh scripts/tcp-evidence.sh --unit
timeout 120 sh scripts/tcp-evidence.sh --audit \
    --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets \
    --freebsd-root /home/bo/work/ravenos-workspace/ravenbsd-resources/freebsd \
    --dragonfly-root /home/bo/work/ravenos-workspace/ravenbsd-resources/dragonfly
timeout 240 sh scripts/tcp-evidence.sh --all \
    --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets \
    --freebsd-root /home/bo/work/ravenos-workspace/ravenbsd-resources/freebsd \
    --dragonfly-root /home/bo/work/ravenos-workspace/ravenbsd-resources/dragonfly
```

`--all` runs the audit, the unit suite and the matrix in one packet. The matrix is recorded as
`blocked` when `scripts/net-conformance.sh` is absent (K253 not merged) or the host cannot create
the tap namespace; it is never recorded as a pass. The sparse donor worktrees are read through
`git show HEAD:<path>` at the pinned revision.

## Intake rule

The NetBSD TCP files carry the NRL 4-clause advertising clause and are **NO-COPY**. A NetBSD
`[[import]]` under `crates/raven-tcpip` is an audit failure. The permitted donors are FreeBSD
`sys/netinet` (BSD-3-Clause, no advertising; `cc/cc_newreno.c` is BSD-2-Clause) and DragonFly
`sys/netinet/tcp_sack.c` (BSD-3-Clause).

A reworked port report sends:

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

The integrator appends the block to `PROVENANCE.toml` and changes the claim from `quarantined` to
`landed` in the same commit. The audit fails if a NetBSD TCP import appears, if a quarantined
claim gains an import, if a landed claim lacks one, if a NetBSD-derived branch/report is not in
the quarantine register, or if the donor file/symbol/licence does not match the pinned donor.
That is what makes the packet an independent audit rather than a summary of the reports.
