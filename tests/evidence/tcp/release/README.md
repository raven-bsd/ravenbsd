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
* `k295/` - current-master audit/unit/checker, Linux logs, measured build logs, RACK audit logs,
  the run manifest and the acceptance record.
* `k295/target/` - current-master matrix.json, target serials, scenario logs, summary and the
  pcap/serial SHA-256 register. The pcaps stay in the generated `run-20261121T052241Z/` packet.
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

Branch `otto/K295` merges current master `2e8a873f` as `4007752f`. The current-master target
matrix passes **10/10** (initfs 158.005 s, image 2.258 s), including the Raven handshake; the
previous `ws=1` finding is not reproduced. The current audit, 1257-test unit suite, checker and
Linux 5/5 are OBSERVED, and the RACK audit is
`donors=219 register=219 accepted=219 gaps=0 duplicate=0`.

The exact `timeout 240 sh scripts/tcp-evidence.sh --all` still exits 124: the measured all-phase
is ~250 s (initfs 174.516 s, image 1.067 s, Linux 5/5, three Raven scenarios). The separate
measured 480 s `--matrix` run is the target evidence; the exact cap is not raised and DONE is not
claimed. K269's code is merged (`f6235c82`) but its K264-owned `PROVENANCE.toml` entry is missing.
The TCP release gate remains **NO-GO**: SSH login, the 24 h soak and an independent audit are not
present.
