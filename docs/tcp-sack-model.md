<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP SACK/DSACK host model (K307)

Status: host-only model, 2026-11-21. **OBSERVED** on the merged K268
`raven-tcpip::SackScoreboard`; no product source is edited here.

## Purpose

The SACK scoreboard is where overlap, DSACK and reneging bugs can silently corrupt the
retransmission decision. `tests/tcp-sack-model/` is an independent byte-set reference and an
adapter for the merged K268 scoreboard. It is a host-only crate; it is not linked into the
stack.

## Model

* `SackedSet` stores individual SACKed byte offsets in a `BTreeSet` and derives merged runs,
  gaps, strict RFC 6675 `IsLost`, pipe and lost bytes from them.
* `relative_offset` maps an absolute TCP sequence number to the scoreboard's
  `snd_una`-relative offset with `wrapping_sub`, so a wrap at `u32::MAX` is covered.
* `board_from_reference` builds a `SackScoreboard` from the reference ranges; the randomized
  test instead calls `insert_block`/`ack_blocks` on both and compares after every operation.
* `classify_dsack` is the DragonFly `tcp_sack_ndsack_blocks` classification: 0 none, 1 DSACK
  below `snd_una`, 2 duplicate out-of-order block enclosed by the next block.

## Coverage matrix

| Requirement | Test |
|---|---|
| overlapping and adjacent merge | `overlapping_and_adjacent_blocks_merge_without_losing_bytes`, `overlapping_blocks_match_the_byte_set_reference` |
| contained/duplicate blocks | `a_block_contained_by_another_is_idempotent`, `duplicate_sack_blocks_are_idempotent` |
| cumulative ACK trim/removal | `cumulative_ack_trims_or_removes_ranges` |
| RFC 6675 IsLost / pipe / lost bytes | `is_lost_uses_the_rfc6675_discontiguous_and_byte_thresholds`, `pipe_and_lost_bytes_are_the_complement_of_sacked_bytes`, random pipe/lost comparison |
| 10k randomized insert/remove | `ten_thousand_random_insert_and_remove_operations_match_the_reference` |
| DSACK classification and skip | `dsack_blocks_are_classified_and_skipped` |
| reneging | `reneging_clears_the_scoreboard` |
| block-count cap | `the_range_cap_is_bounded_and_reports_drops`, `a_full_scoreboard_drops_an_overlapping_block_before_merging` |
| receiver block order / option cap | `receiver_block_order_is_dsack_first_then_newest_with_the_option_cap` |
| sequence wrap | `sequence_wrap_maps_to_relative_offsets` |

Random seed: `0x30705a434b534143` (`0x3070_5a43_4b53_4143`). The 10k run performs 8002
inserts and 1901 complete-block removals; the remaining steps have no block to remove. It
compares merged ranges and gaps after every step and pipe/lost ranges every 100 steps.

## Known K268 findings (reported, not patched here)

1. `SackScoreboard::sacked_bytes_above(point)` counts a block byte at `point`; the reference
   counts strictly above. The K255 report recorded `100` vs `99` for `[200,300)` at point
   `200`. The model test `the_reference_byte_count_is_strictly_above_the_point` pins the
   reference side.
2. `insert_block` checks `range_cap` before merging, so a block that would merge into an
   existing range is dropped when the board is full. The test
   `a_full_scoreboard_drops_an_overlapping_block_before_merging` characterizes this K268
   behavior; the expected repair belongs to K268.

## Donor and references

* DragonFly `sys/netinet/tcp_sack.c` @ `94448f622a06cde90ba42d688e9cab2448ad9ab3`
  (3-clause BSD): `tcp_sack_ndsack_blocks`, `tcp_sack_add_blocks`, `tcp_sack_ack_blocks`,
  `tcp_sack_update_lostseq`, `tcp_sack_islost`, `tcp_sack_compute_pipe`,
  `tcp_sack_first_unsacked_len`, `tcp_sack_fill_report`, `tcp_sack_update_reported_history`.
  Behaviour only; no C expression was copied.
* RFC 6675 sections 3-5 (SACK scoreboard, IsLost, SetPipe).
* Merged K268 implementation: `crates/raven-tcpip/src/sack.rs`.

## Run

```sh
timeout 180 cargo nextest run -p tcp-sack-model
timeout 420 make check-fast
```
