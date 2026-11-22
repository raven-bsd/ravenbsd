<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK audit freeze (K366, refreshed by K369)

Frozen full-RACK acceptance evidence from a clean worktree at the K369 tool commit.

## Inputs

| Item | Value |
|---|---|
| tool commit | `59c7a9ad7df6d368de768b106ab9c3ea0990d882` |
| base master | `ddda93a5d9288db7a22eff2c2c8e0039727689a7` |
| tool | `rack-acceptance` at `59c7a9ad` |
| register | `docs/rack-coverage.md`, 219 rows, 219 PASS |
| register SHA-256 | `d39d94a729354f810b0f6397f5a1a5e99ec4844182fbbd6df7ba6f4dc8f5c22e` |
| donor | FreeBSD `sys/netinet/tcp_stacks/rack.c` at `133344405f532bd54ff3629a77d4c4e062f01fdb` |
| donor extract | `/tmp/opencode/freebsd-rack.c` |
| donor SHA-256 | `8c4a0b28b340216ac5de99c41dcb55f562439ee02314ca59a3ff9f2002b6d09e` |
| donor functions | 219 |

The clean worktree is `/tmp/opencode/k369/clean`, checked out at the tool commit.

## Green run

```text
cargo run -q -p rack-acceptance -- \
  --reports ../ravenbsd-prjmgmt/tickets --donor /tmp/opencode/freebsd-rack.c
exit 0
acceptance: donors=219 register=219 target=218 accepted=219 gaps=0 missing=0 duplicate=0 unknown=0
k315: missing=0 duplicate=0 unknown=0 empty_rust=0 empty_test=0 empty_mutation=0 not_pass=0
target note: ticket text says 218 but the pinned donor/manifest has 219; the extra row is reported, not dropped
218-row view: 218/218 if the one-row discrepancy excludes tcp_addrack (PASS)
```

Full output: `tests/rack-audit-freeze/acceptance.log`, runtime 3.28 s.

## Mutation / negative check

All mutated runs use `/tmp/opencode/k369/mut`, a scratch worktree at the same tool
commit; the real register and reports were not modified.

1. **Blank test cell** (`rack_log_gpset` test -> `-`): `exit 1`,
   `accepted=219`, `k315: empty_test=1`. Log: `blank-test.log`.
2. **Blank Rust cell** (`rack_log_gpset` Rust -> `-`): `exit 1`,
   `accepted=219`, `k315: empty_rust=1`. Log: `blank-rust.log`.
3. **Blank mutation cell** (`rack_log_gpset` mutation -> `-`): `exit 1`,
   `accepted=218 gaps=1`, `k315: empty_mutation=1`. Log: `blank-mutation.log`.
4. **Remove one accepted row** (`tcp_addrack`): `exit 1`,
   `donors=219 register=218 accepted=218 missing=1`. Log: `remove-row.log`.
5. **Remove one mapping** (`rack_log_gpset` Rust -> `missing_mapping`): `exit 1`,
   `accepted=218 gaps=1`. Log: `mapping-removal.log`.

K366 recorded the blank-test run as a finding (`exit 0`, `empty_test=1`). K369
fixed `rack-acceptance::is_ok` to require the K315 cell audit clean, so the same
mutation now exits 1. No check was weakened.

## Machine-readable fixture

`tests/rack-audit-freeze/freeze.json` records the tool commit, donor/register
hashes, commands, exits, summaries and logs. `donor.sha256` pins the extract.

## Scope

No product code, register, replay tool or ticket report was changed. The audit's
replay section still reports model gaps for BBR/TLP/loss (the product exposes
only part of those models); those are not register gaps and do not change the
219/219 acceptance result.

## Reproduction

```text
rm -rf /tmp/opencode/k369/clean
git worktree add --detach /tmp/opencode/k369/clean 59c7a9ad7df6d368de768b106ab9c3ea0990d882
cd /tmp/opencode/k369/clean
timeout 120 cargo run -q -p rack-acceptance -- \
  --reports ../ravenbsd-prjmgmt/tickets --donor /tmp/opencode/freebsd-rack.c
```

The donor extract must have the SHA-256 above; the manifest count is 219.
