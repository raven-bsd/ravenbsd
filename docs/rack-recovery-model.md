<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK recovery/pipe reference model (K324)

Host-only oracle for the full RACK recovery/pipe layer: recovery entry/exit, lost ranges, the RFC
6675 pipe and retransmit selection. It extends the merged K320 `rack-tlp-model` `RackModel` for the
shared send-map, TLP, DSACK, reordering-window and loss-by-time core; K324 adds only the recovery
and pipe layer. No product source is edited.

## Matrix

| Test | Assertion |
|---|---|
| `the_10k_golden_trace_regenerates_exactly` | seeds 1..=10000 regenerate the committed TSV byte-for-byte; every row has 7 fields, pipe <= sent stream, exits <= entries and sorted in-range lost ranges |
| `recovery_entry_exit_and_pipe_are_exact` | 8 segments, SACK the last: lost `1000-8000`, pipe `1000`, retransmit `1000-2000`, entries/exits `1/0`; a full ACK exits and pipe becomes `0` |
| `the_shared_core_matches_the_product_loss_bound` | the K320 `RackModel.is_lost` bound equals the current product `Rack::is_lost` at 100 ms and 20 ms |
| `a_corrupted_golden_row_is_detected` | a changed pipe field makes the trace comparison fail |

## Model

* Recovery entry on the first lost range; exit when the cumulative ACK reaches the recovery point.
* Lost ranges are segment-granular, sorted and merged.
* Pipe: `outstanding - sacked - lost + retransmitted`, saturating; the retransmitted term counts
  only the selected bytes still between `snd_una` and `snd_max`.
* Retransmit selection: the lowest lost segment not yet selected, one per event; the segment's
  send-map retransmit count and the K320 recovery byte counter are incremented.

The shared K320 `RackModel` supplies `is_lost`, `sacked`, `enter_recovery`/`recovery` exit, TLP,
DSACK and reordering; K324 does not reimplement them.

## Trace format (shared with K319)

`traces/rack-recovery-10k.tsv`:

```text
# seed	events	lost	pipe	retransmits	recovery_entries/exits	event_digest
```

* `lost` and `retransmits`: comma-separated `start-end` byte ranges, or `-`.
* `event_digest`: FNV-1a 64-bit digest of the canonical event list.
* Rows: 10,000 (`seed=1..10000`); SHA-256 of the golden file:
  `1fed5918dff8cfef3b85719f4a6574aa06dee8ef0a068a6c681c7627c98cb812`.

Regenerate:

```sh
timeout 120 cargo run -q -p rack-recovery-model -- \
  --generate tests/rack-recovery-model/traces/rack-recovery-10k.tsv
```

## Mutation / negative

A temporary model mutation removed the `+ retransmitted` term from the pipe formula; the 10k
golden test fails (`the golden trace is stale`) and the file is restored byte-identically. The
corrupted-row test is the direct trace-comparison negative.

## Run

```sh
timeout 180 cargo nextest run -p rack-recovery-model
timeout 420 make check-fast
```

## Donor

FreeBSD `sys/netinet/tcp_stacks/rack.c` at `133344405f532bd54ff3629a77d4c4e062f01fdb`
(`rack_exit_recovery`, `rack_cong_signal`, `rack_process_to_cumack`, sendmap retransmits) and
RFC 8985/6675. The K320 model is reused; K319's trace format is reproduced. No donor code copied.

## Open gaps

K319's crate is not merged; its TSV format is reproduced from the commit. The current product
`Rack` exposes only the loss-by-time/reordering window, so the full product-to-trace comparison is
the K269 gate; K324 supplies the exact oracle now.
