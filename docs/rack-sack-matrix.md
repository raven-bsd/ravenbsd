<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK SACK/DSACK test matrix (K347)

Status: host-only evidence matrix over the merged K336 SACK/DSACK/reneging module. **OBSERVED**
by `cargo nextest run -p rack-sack-matrix`; no product source is changed.

## What it runs

`tests/rack-sack-matrix` calls every public `RackSack` path and compares the observable state with
an independent range reference (`src/reference.rs`):

| Case | Product path | Reference / oracle |
|---|---|---|
| SACK split/merge/ACKED flags | `rack_proc_sack_blk` | range merge plus sendmap flag checks |
| SACK-passed loss and stop | `rack_log_sack_passed` | threshold/`WAS_LOST`/`SACK_PASSED` exact vectors |
| peer reneging | `rack_peer_reneges` | `trim_below` and filter clear |
| cumack prune/trim/advance | `rack_process_to_cumack` | `trim_below`, `SND.UNA` check |
| DSACK TLP exclusion/round/spurious | `rack_note_dsack` | K320 rules and exact counters |
| DSACK log words | `rack_log_dsack_event` | K314 four-word event |
| SACK filter | `sack_filter_blks`/`sack_filter_clear` | merged range filter |
| K315 group rows | `rack-functions.txt` | six names exactly once |

## Trace seeds

The matrix reads the committed K319 and K324 traces and reuses their seed columns:

```text
tests/rack-loss-model/traces/rack-10k.tsv               10,000 seeds
tests/rack-recovery-model/traces/rack-recovery-10k.tsv  10,000 seeds
```

Each seed drives 12 deterministic SACK/DSACK/cumack/renege events on the product and the
independent reference; the matrix compares sacked ranges, sacked bytes, `SND.UNA`, DSACK rounds,
DSACK bytes, persist and sorted/disjoint-range invariants after every event. Total: **20,000
scenarios**.

K320 supplies the DSACK rules (TLP-caused DSACK excluded, one round, persist 16, spurious
recovery). K326 (BBR/pacing) and K327 (transport vtable) are different domains; this matrix does
not duplicate their models.

## Canonical SACK flag bits

K349/K364 unified `rack_sack.rs` with the canonical `rack_types.rs` flags. The matrix now imports
the public constants and asserts the sendmap uses them:

| Flag | Canonical value |
|---|---:|
| `RACK_ACKED` | 0x000001 |
| `RACK_SACK_PASSED` | 0x000010 |
| `RACK_WAS_SACKPASS` | 0x000020 |
| `RACK_WAS_LOST` | 0x400000 |
| `RACK_RWND_COLLAPSED` | 0x000100 |

`k336_flag_bits_match_public_rack_constants` verifies the product and matrix agree; the previous
K347 mismatch finding is resolved by the canonical type migration.

## Donors

FreeBSD `sys/netinet/tcp_stacks/rack.c` at `133344405f532bd54ff3629a77d4c4e062f01fdb`
(BSD-2-Clause): `rack_log_dsack_event` 2405, `rack_log_sack_passed` 9020, `rack_proc_sack_blk`
9280, `rack_peer_reneges` 10055, `rack_process_to_cumack` 10182, `rack_note_dsack` 10602.
Illumos has no RACK module; Linux is outcome-only. This ticket copies no donor code.

## Running

```sh
timeout 180 cargo nextest run -p rack-sack-matrix
timeout 420 make check-fast
```

The acceptance report and K315 union row are in `tickets/tim/reports/K347.md`.
