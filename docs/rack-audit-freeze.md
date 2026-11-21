<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK audit freeze (K366)

Frozen full-RACK acceptance evidence from a clean clone of `origin/master`.

## Inputs

| Item | Value |
|---|---|
| master commit | `2e8a873fd3a4e87b04d315cc9f1651f66b5364b1` |
| tool | `rack-acceptance` at the same commit |
| register | `docs/rack-coverage.md`, 219 rows, 219 PASS |
| register SHA-256 | `d39d94a729354f810b0f6397f5a1a5e99ec4844182fbbd6df7ba6f4dc8f5c22e` |
| donor | FreeBSD `sys/netinet/tcp_stacks/rack.c` at `133344405f532bd54ff3629a77d4c4e062f01fdb` |
| donor extract | `/tmp/opencode/freebsd-rack.c` |
| donor SHA-256 | `8c4a0b28b340216ac5de99c41dcb55f562439ee02314ca59a3ff9f2002b6d09e` |
| donor functions | 219 |

The fresh clone is `/tmp/opencode/k366/fresh`, cloned with
`git clone --branch master --single-branch` from the minion clone and checked
out at `2e8a873f`.

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

Full output: `tests/rack-audit-freeze/acceptance.log`, runtime 4.57 s.

## Mutation / negative check

All mutated runs use a scratch copy of the clean clone; the real register and
reports were not modified.

1. **Remove one accepted row** (`tcp_addrack`) from the scratch register:
   `exit 1`, `donors=219 register=218 accepted=218 missing=1 gaps=0`.
   Log: `remove-row.log`.
2. **Remove one mapping** (`rack_log_gpset` Rust cell -> `missing_mapping`):
   `exit 1`, `accepted=218 gaps=1 missing=0 duplicate=0 unknown=0`.
   Log: `mapping-removal.log`.
3. **Blank one test cell** (`rack_log_gpset` test -> `-`): the audit exits
   **0** but K315 reports `empty_test=1`. This is a finding: the current
   `rack-acceptance::is_ok` checks missing/duplicates/unknown/accepted but not
   the K315 empty-cell lists. Log: `blank-test.log`. This ticket does not edit
   the tool.

The required "remove one accepted row" proves the audit detects the missing
function with a non-zero exit. The mapping-removal run additionally produces
`gaps=1`; the blank-test run exposes a tool gap for its owner.

## Machine-readable fixture

`tests/rack-audit-freeze/freeze.json` records the commit, donor/register hashes,
commands, exits, summaries and logs. `donor.sha256` pins the extract.

## Scope

No product code, register, acceptance tool, replay tool or ticket report was
changed. The audit's replay section still reports model gaps for BBR/TLP/loss
(the product exposes only part of those models); those are not register gaps and
do not change the 219/219 acceptance result.

## Reproduction

```text
rm -rf /tmp/opencode/k366/fresh
git clone --branch master --single-branch <clean-clone> /tmp/opencode/k366/fresh
cd /tmp/opencode/k366/fresh
timeout 120 cargo run -q -p rack-acceptance -- \
  --reports ../ravenbsd-prjmgmt/tickets --donor /tmp/opencode/freebsd-rack.c
```

The donor extract must have the SHA-256 above; the manifest count is 219.
