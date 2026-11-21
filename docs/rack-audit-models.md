<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK model-group audit (K358)

Status: host-only static audit of the merged independent RACK models. **OBSERVED** by
`cargo nextest run -p rack-audit-models`; no product source is changed.

## Scope

The audited group is:

| Crate | Model | Report |
|---|---|---|
| `tests/rack-loss-model` | K319 loss/reordering | K319 |
| `tests/rack-tlp-model` | K320 TLP/DSACK | K320 |
| `tests/rack-recovery-model` | K324 recovery/pipe | K324 |
| `tests/rack-bbr-model` | K326 BBR/pacing/timely | K326 |
| `tests/rack-vtable-model` | K327 transport vtable | K327 |

The audit manifest (`tests/rack-audit-models/audit.tsv`) pins 19 donor function/value rows:
group, donor name, model crate/source/token, named test, mutation evidence and whether the donor is
in the K315 219-function manifest. `constants.tsv` pins donor constants; `mutations.tsv` records the
group reports' mutation evidence.

## Checks

* Each model crate/source/token exists; each named test exists in the crate.
* Each mutation claim is recorded in `mutations.tsv`.
* Each `k315=yes` donor appears exactly once in `tests/rack-coverage/fixtures/rack-functions.txt`.
* Donor constants: `BBR_UNIT=256`, `TCP_FUNC_OUTPUT_CANDROP=2`, `TCP_FUNC_DEFAULT_OK=4`,
  `RL_MIN_DIVISOR=50`, `TCP_RACK_PACE_MAX_SEG=1054`, `TCP_RACK_PACING_DIVISOR=1146`,
  `TCP_RACK_PACE_MIN_SEG=1147`, `RACK_RXT_SCOREBOARD_CLEAR_THRESH=2`.
* K319 and K324 traces each have 10,000 data rows; K320/K326/K327 have their randomized tests.
* The K315 manifest has 219 unique functions.

## Exact findings

1. **K327 vtable flags differ from the product.** The model uses the donor values
   `TCP_FUNC_OUTPUT_CANDROP=0x02`, `TCP_FUNC_DEFAULT_OK=0x04`, `tfb_flags=6`; the merged
   `crates/raven-tcpip/src/rack_vtable.rs` uses `0x0001`/`0x0002`, so the product flags are `3`.
   `docs/rack-replay.md` records the expected `6`, got `3` replay mismatch. Finding owner: K318.
2. **K319/K324 replay gaps.** `docs/rack-replay.md` records loss seed 1 expected pipe 2000, got
   1000; recovery seed 11 expected pipe 1000, got 0. The models add the selected retransmission
   back to pipe while the merged `SackScoreboard` does not. Finding owner: K269.
3. **K320 reo_wnd replay gap.** `docs/rack-replay.md` records TLP seed `0x32000001` event 192:
   expected `reo_wnd 25`, got `50`; the product `Rack::on_dsack` grows on every call. Finding
   owner: K269/K320.
4. **K327 mutation breadth.** K327's mutation is one vtable-wide `switch_failed` no-op, not a
   per-callback mutation. The audit rows use that mutation for the vtable group and note this as a
   weak per-callback mutation gap for K327.
5. **K315 union.** The six K336 SACK rows are the only current group rows in the K315 subset; the
   K315 CLI reports 213 donor functions still missing across K269/K328-K335. The full union is not
   yet green; the audit records the exact missing count rather than claiming it.

## Running

```sh
timeout 180 cargo nextest run -p rack-audit-models
timeout 180 cargo nextest run -p rack-loss-model -p rack-tlp-model -p rack-recovery-model -p rack-bbr-model -p rack-vtable-model
timeout 420 make check-fast
```

Evidence and commands are in `tickets/tim/reports/K358.md`.
