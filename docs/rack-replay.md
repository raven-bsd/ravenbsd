<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK full-port replay and union audit (K337)

Status: host-only evidence tooling. **OBSERVED** by `cargo nextest run -p rack-replay` and
`cargo run -p rack-replay -- --replay`; no product source is edited.

## Purpose

The RACK port is split across K269 and K328-K336. The acceptance is the union, not one report.
This crate has two jobs:

1. replay the independent model traces from K319/K320/K324/K326/K327 against the ported
   `Rack`/`SackScoreboard`/`Tlp`/`RackStack` primitives and record the first mismatch per class;
2. merge the K269/K328-K336 coverage matrices and run the K315 checker on the union.

A red trace is a finding for the owning group. It is not a replay-harness failure and the
harness never weakens the oracle to turn it green.

## Trace classes

| Class | Oracle | Input | Product path |
|---|---|---|---|
| `loss` | K319 `rack-loss-model` | committed `tests/rack-loss-model/traces/rack-10k.tsv`, 10,000 rows | `SackScoreboard` (lost ranges and pipe) |
| `recovery` | K324 `rack-recovery-model` | committed `tests/rack-recovery-model/traces/rack-recovery-10k.tsv`, 10,000 rows | `SackScoreboard` (lost ranges and pipe) |
| `tlp` | K320 `rack-tlp-model` | 5 seeds x 2,000 deterministic events (`0x32000001..05`) | `Tlp` and `Rack` |
| `bbr` | K326 `rack-bbr-model` | 10 seeds x 1,000 deterministic events (`0x32600001..0a`) | absent in `raven-tcpip` |
| `vtable` | K327 `rack-vtable-model` | 5 seeds x 2,000 deterministic events (`0x32700001..05`) plus the 19-callback probe matrix | `RackStack` (`RackVtable`) |

K319 and K324 commit their 10k rows as a TSV. K320/K326/K327 keep their 10k-event traces in
code, so the replay re-runs the same seeds through their public model APIs and compares the
product primitive on the events it exposes.

The committed rows are checked twice: the model must regenerate the recorded digest and outcome
(model-regression check), then the product primitive must match that outcome (port check). The
first replay mismatch is the port finding; a digest mismatch is a stale/corrupt golden file.

## Running

```sh
timeout 180 cargo nextest run -p rack-replay
timeout 120 cargo run -q -p rack-replay -- --replay
timeout 120 cargo run -q -p rack-replay -- --union --reports <tickets-directory>
```

`--union` loads `K269.md` and `K328.md`-`K336.md` from `<tickets-directory>/<lane>/reports/`.
It uses the pinned 219-function manifest in `tests/rack-coverage/fixtures/rack-functions.txt`
by default; pass `--donor <rack.c>` to parse the real donor file instead.

The replay command exits 0 even when a class is red: the mismatch is evidence for the owning
group. The union command exits non-zero when a function is missing, duplicated, unknown, empty
or not `PASS`, which is the acceptance gate for K269.

## Replay matrix (2026-11-21, origin/master 6266153f)

| Class | Traces | Events | Mismatches | First mismatch |
|---|---:|---:|---:|---|
| `loss` | 10,000 | 105,000 | 7,948 | seed 1: expected lost `4000-5000` pipe `2000`, got lost `4000-5000` pipe `1000` |
| `recovery` | 10,000 | 121,925 | 1,025 | seed 11: expected lost `-` pipe `1000`, got lost `-` pipe `0` |
| `tlp` | 5 | 10,000 | 1,747 | seed `0x32000001` event 192: expected `reo_wnd 25`, got `reo_wnd 50` |
| `bbr` | 10 | 10,000 | 10,000 | seed `0x3260000a`: expected pacing rate/cwnd, got absent |
| `vtable` | 5 | 10,000 | 1 | `tfb_flags`: expected `6` (`TCP_FUNC_OUTPUT_CANDROP|TCP_FUNC_DEFAULT_OK`), got `3` |

The loss/recovery pipe differences are the RFC 6675 `SackScoreboard` accounting versus the
K319/K324 recovery model (the models add the selected retransmission back to pipe and use a
time-based RACK loss bound). The `recovery_entries/exits` and retransmit lists are not exposed
by the current public scoreboard result, so they are recorded as gaps rather than silently
treated as equal.

The TLP difference is the DSACK reordering-window growth: K320 grows once per DSACK round and
ignores TLP-caused DSACKs; `Rack::on_dsack` grows on every call and caps at SRTT. The product
`Tlp` also has no two-probe counter, no RTO cap and no spurious-undo/reneging result.

BBR/timely/pacing has no product engine at all.

The vtable finding is a literal constant mismatch: the product `RackFunctionBlock` defines
`TCP_FUNC_OUTPUT_CANDROP = 0x0001` and `TCP_FUNC_DEFAULT_OK = 0x0002`; FreeBSD
`sys/netinet/tcp_var.h:533-534` and K327 define `0x02` and `0x04`. The other vtable probes
(init/fini, `PRUS_OOB`, pacing divisor, pipe, timers query, handoff, retransmit scoreboard clear)
match.

## Union audit (K315)

Snapshot 2026-11-21T02:54:00Z (the group reports are live while K328-K336 are still working):

```text
union: donors 219 rows 219 groups K269,K328,K329,K330,K331,K332,K333,K334,K335,K336
missing=0 duplicate=0 unknown=0 empty_rust=107 empty_test=107 empty_mutation=107
not_pass=107 cross_report_duplicate=55
```

* No donor function is missing and no name is duplicated inside the selected union.
* 107 rows are still unmapped/BLOCKED (K269's incomplete inventory).
* 55 functions are claimed by more than one group with a substantive mapping
  (`rack_calc_thresh_rack`, `rack_note_dsack`, `rack_proc_sack_blk`, `rack_process_to_cumack`,
  `tcp_rack_output`, most `rack_log_*` helpers, ...). That is a union ownership defect: exactly
  one group must own each donor function.

The counts move as the other lanes commit; rerun the command for the current snapshot. Using the
pinned manifest or the real donor (`--donor`) gives the same result.

K269 remains the blocker: K337 does not port product code and does not mark a BLOCKED row
`PASS`.

## Donor reconciliation

Test/evidence only. FreeBSD `sys/netinet/tcp_stacks/rack.c`, `sys/netinet/tcp_var.h` and
`sys/netinet/tcp_stacks/bbr.c` at `133344405f532bd54ff3629a77d4c4e062f01fdb`
(BSD-2-Clause) are read for the function list and behaviour. The replay is original Rust and
copies no donor expression. Illumos is CDDL and Linux is GPL: no RACK/BBR module is copied or
translated from either. The K319/K320/K324/K326/K327 models are first-party test code; the K315
checker and manifest are reused as the acceptance tool.

## Limits

* The product comparison uses the public `SackScoreboard`, `Tlp`, `Rack` and `RackStack`
  primitives. It does not instantiate a full `Stack`; K269 owns the sendmap integration.
* Loss/recovery compare lost ranges and pipe only; the model's retransmit and recovery counters
  are recorded as product gaps.
* The vtable class compares the callback probe matrix; the 10k-event K327 trace is replayed for
  determinism and format only.
* No product edits, no test weakening, no skipped class.
