<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP release evidence packet (K295)

This directory is the machine-readable TCP release section. It is owned by K295; the K264
automation and packets are the inputs.

## Layout

* `manifest.json` - one OBSERVED row per release claim: `id`, `claim`, `tag`, `artifact`,
  `sha256`; plus the eight port-report rows under `reports`.
* `check.py` - validates every tag, artifact path and SHA-256; `--selftest` proves a missing tag,
  missing artifact and wrong hash are rejected.
* `k295/` - the current-master audit/unit/checker copies, Linux matrix logs, the run manifest and
  the target-revalidation blocker record.
* `../run-20261121T003615Z/` - the committed K264 matrix packet (target pcaps/serials, build logs).
* `../run-20261121T004851Z/` - the K264 clean audit/unit/checker packet.

## Tags

Every manifest row is **OBSERVED**: it has a measured artifact in the tree and a SHA-256. The
packet no longer carries PROPOSED rows; unfinished work is listed in `docs/release-evidence-tcp.md`
under Open gaps and is not asserted as release evidence.

## Validate

```sh
timeout 30 python3 tests/evidence/tcp/release/check.py --selftest
timeout 30 python3 tests/evidence/tcp/release/check.py --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets
```

The report rows use paths relative to `tickets/`; without `--reports-root` their hashes are
reported as warnings, not failures.

## Current state

Branch `otto/K295` merges current master `eee05a07` as `e033b230`. The current-master audit,
1156-test unit suite, checker and Linux matrix 5/5 are OBSERVED. The current-master target matrix
is BLOCKED: `--all` timed out at 240 s and a 600 s `--matrix` warm run timed out too, both during
the target initfs build, with the host at load 127.51 and 11 concurrent minion check-fast runs. No
target `matrix.json` exists for the merge, so the committed K264 packet remains the durable target
wire evidence. The Raven handshake window-scale finding is OBSERVED; its fix is an open gap. This
packet is not a release sign-off.
