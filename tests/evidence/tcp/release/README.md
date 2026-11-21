<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP release evidence packet (K295)

This directory is the machine-readable TCP release section. It is owned by K295; the K264
automation and packets are the inputs.

## Layout

* `manifest.json` - one row per release claim: `id`, `claim`, `tag` (`OBSERVED`/`PROPOSED`),
  `artifact`, `sha256`; plus the port-report rows under `reports`.
* `check.py` - validates every tag, artifact path and SHA-256; `--selftest` proves a missing tag,
  missing artifact and wrong hash are rejected.
* `k295/` - the current K295 audit/unit/checker copies and Linux matrix logs.
* `../run-20261121T003615Z/` - the committed K264 matrix packet (target pcaps/serials, build logs).
* `../run-20261121T004851Z/` - the K264 clean audit/unit/checker packet.

## Tags

**OBSERVED** rows have a measured artifact in the tree. **PROPOSED** rows are release work that is
not yet complete (target revalidation with a measured cap, the handshake window-scale fix, the
quarantined ports).

## Validate

```sh
timeout 30 python3 tests/evidence/tcp/release/check.py --selftest
timeout 30 python3 tests/evidence/tcp/release/check.py
timeout 30 python3 tests/evidence/tcp/release/check.py \
    --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets
```

The report rows use paths relative to `tickets/`; without `--reports-root` their hashes are
reported as warnings, not failures.

## Current state

The K295 `--all` attempt completed the audit, unit, checker and Linux matrix, then hit the ticket's
240 s cap during the target initfs build (the K264 packet measured 431.881 s initfs + 15.668 s
image). The target matrix is PROPOSED for a measured-cap rerun; the K264 packet remains the
committed wire evidence. The Raven handshake window-scale finding is OBSERVED; the fix is
PROPOSED. This packet is not a release sign-off.
