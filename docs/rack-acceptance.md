<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK 218/218 acceptance audit (K350)

Status: host-only evidence tooling. **OBSERVED** by `cargo nextest run -p rack-acceptance` and
`cargo run -p rack-acceptance -- --reports <tickets>`. No product source is edited.

## What the audit proves

The pinned FreeBSD `sys/netinet/tcp_stacks/rack.c` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` has **219** top-level functions. The K350 ticket
says 218; the manifest, parser and canonical register all report 219. The audit reports the exact
219-row status and names the one-row discrepancy rather than dropping a function.

For every donor function the audit requires:

1. a canonical owner row in `docs/rack-coverage.md`;
2. a Rust mapping that exists in the current `crates/raven-tcpip` source;
3. a named `#[test]` that exists in the current source;
4. a durable per-row mutation/negative check.

The audit also runs the K315 checker over the canonical register and the K337 replay over the
K319/K320/K324/K326/K327 traces.

## Current result (2026-11-21T03:54:38Z, origin/master eee05a07)

```text
acceptance: donors=219 register=219 target=218 accepted=146 gaps=73 missing=0 duplicate=63 unknown=0
k315: missing=0 duplicate=0 unknown=0 empty_rust=174 empty_test=174 empty_mutation=174 not_pass=174
target note: ticket text says 218 but the pinned donor/manifest has 219; the extra row is reported, not dropped
218-row view: 145/218 if the one-row discrepancy excludes tcp_addrack (PASS)
```

**The port is not accepted at 218/218.** The exact 219-row status is 146 accepted and 73 gaps.
No donor function is missing or unknown; 63 functions are claimed by more than one group.

The K315 checker over the canonical register reports 174 `not PASS` because the register has not
been refreshed after the group merges; the source audit is the stricter, current status.

## Owner split

| Owner | Pass | Total | Gap summary |
|---|---:|---:|---|
| K269 | 0 | 2 | `rack_cong_signal`, `rack_update_prr`: the old `Rack::*` names no longer exist |
| K328 | 35 | 35 | none |
| K329 | 43 | 43 | none |
| K330 | 15 | 15 | none |
| K331 | 13 | 15 | `rack_need_set_test`, `rack_check_reorder_ack` |
| K332 | 15 | 15 | none |
| K333 | 11 | 11 | none |
| K334 | 7 | 76 | 34 have mapping/test but no durable per-row mutation; 2 have no named test; 33 have no mapping/test/mutation |
| K335 | 3 | 3 | none |
| K336 | 4 | 4 | none |

K269's two rows are a real divergence: K333 inlined the `rack_cong_signal` CC_NDUPACK entry into
`rack_pipe::rack_enter_recovery` and records that divergence; the canonical register still points
at `Rack::cong_signal`. `rack_update_prr` has no current symbol. The lead must either restore the
names, update the register/mapping, or record an approved divergence.

K334's tests and functions exist, but the K334 report's per-row K315 matrix was written to
`/tmp/opencode/k334-matrix.md` and is not durable. The audit therefore reports 34 rows with
mapping/test but no mutation, 2 rows with no named test (`rack_fini`, `rack_dtor`), and 33 rows
with no mapping/test (e.g. `rack_swap_beta_values`, `rc_init_window`, `rack_update_multiplier`,
`rack_validate_multipliers_*`, `rack_cc_after_idle`, `tcp_rack_partialack`, `rack_fast_output`,
`rack_new_round_*`).

## Duplicate claims

63 donor functions are claimed by more than one group with a substantive mapping. The largest
overlap is K269's historical matrix versus the current group reports (`rack_ack_received`,
`rack_calc_thresh_rack`, `rack_calc_thresh_tlp`, `rack_log_*`, `rack_note_dsack`,
`rack_peer_reneges`, `rack_proc_sack_blk`, `rack_process_to_cumack`, `tcp_rack_output`, ...).
K338 already flagged K336/K329 and K335/K334 overlaps. Exactly one group must own each row.

## K337 replay matrix

| Class | Traces | Events | Mismatches | First mismatch |
|---|---:|---:|---:|---|
| `loss` | 10,000 | 105,000 | 7,948 | seed 1: pipe `2000` vs `1000` |
| `recovery` | 10,000 | 121,925 | 1,025 | seed 11: pipe `1000` vs `0` |
| `tlp` | 5 | 10,000 | 1,747 | DSACK `reo_wnd 25` vs `50` |
| `bbr` | 10 | 10,000 | 10,000 | no BBR/timely/pacing engine in `raven-tcpip` |
| `vtable` | 5 | 10,000 | 1 | `tfb_flags` `6` vs `3` |

These are findings for K319/K324/K320/K326/K327, not audit-harness failures. The K337 report has
the detailed per-class evidence.

## Commands

```sh
timeout 180 cargo nextest run -p rack-acceptance
timeout 120 cargo run -q -p rack-acceptance -- --reports <tickets-directory>
timeout 120 cargo run -q -p rack-acceptance -- --reports <tickets-directory> --rows
timeout 120 cargo run -q -p rack-acceptance -- --donor /path/to/rack.c
```

`--rows` prints the exact 219-row table with the mapping/test/mutation facts.

## Donor reconciliation

Test/evidence only. FreeBSD `sys/netinet/tcp_stacks/rack.c`,
`sys/netinet/tcp_stacks/tcp_rack.h`, `sys/netinet/tcp_var.h` and
`sys/netinet/tcp_stacks/bbr.c` at `133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause) are
read for the function list and behaviour. The audit is original Rust and copies no donor code.
Illumos is CDDL and Linux is GPL: no RACK code is copied or translated from either.

## Limits

* The audit verifies symbols by exact function name, the donor name and the stripped
  `rack_`/`tcp_` form. A function renamed without a report row is a gap, not an inference.
* A per-row mutation is accepted only from a durable report/register cell; prose module-level
  mutation evidence is not enough.
* The 218/219 discrepancy is reported, not resolved; the extra row is `tcp_addrack` (K335).
