<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP release evidence packet (K295)

This directory is the machine-readable TCP release section. It is owned by K295; the K264
automation and packets are the inputs.

## Layout

* `manifest.json` - one OBSERVED row per release claim: `id`, `claim`, `tag`, `artifact`,
  `sha256`; plus the port-report rows under `reports`.
* `check.py` - validates every tag, artifact path and SHA-256; `--selftest` proves a missing tag,
  missing artifact and wrong hash are rejected.
* `k295/` - current-master audit/unit/checker, measured build logs, RACK audit logs, the run
  manifest and the 360 s acceptance record.
* `k295/target/` - current-master matrix.json, target serials, scenario logs, summary and the
  pcap/serial SHA-256 register. The pcaps stay in the generated `run-20261121T053533Z/` packet.
* `../run-20261121T003615Z/` - the committed K264 matrix packet (historical target pcaps/serials).
* `../run-20261121T004851Z/` - the K264 clean audit/unit/checker packet.

## Tags

Every manifest row is **OBSERVED**: it has a measured artifact in the tree and a SHA-256. The
packet has no PROPOSED rows; unfinished work is listed in `docs/release-evidence-tcp.md` under
Open gaps and is not asserted as release evidence.

## Validate

```sh
timeout 30 python3 tests/evidence/tcp/release/check.py --selftest
timeout 30 python3 tests/evidence/tcp/release/check.py --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets
```

The report rows use paths relative to `tickets/`; without `--reports-root` their hashes are
reported as warnings, not failures.

## Current state

Branch `otto/K295` merges current master `269d43dc` as `e7b21a24`. The lead-authorized
`timeout 360 sh scripts/tcp-evidence.sh --all` exits 0: audit pass (134 files, 9 imports,
0 violations), unit 1262/1262, checker pass, matrix **10/10**, initfs 192.361 s, image 1.937 s,
clean tree. The Raven handshake passes with `ws=3` only when the peer offers; the previous `ws=1`
finding is not reproduced. The RACK audit is `donors=219 register=219 accepted=219 gaps=0
duplicate=0`, with K315 empty cells 0.

K269's code is merged (`f6235c82`) but its K264-owned `PROVENANCE.toml` entry is still missing;
this is an open gap. The TCP release gate remains **NO-GO**: the matrix is present, but SSH login,
the 24 h soak and an independent audit are not.
