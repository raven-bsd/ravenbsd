# RACK-TLP / DSACK / spurious-undo model

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Host-only independent model for the RACK edge paths in the full K269 port. The package is
`tests/rack-tlp-model` (`cargo nextest run -p rack-tlp-model`); it never edits product source.
It is the oracle for the future full `Rack`, not a second copy of the current four-field detector.

## Model

| Case | Model expectation | Test |
|---|---|---|
| TLP PTO | `SRTT + max(4*RTTVAR,1) + 2`, capped by RTO | `tlp_pto_and_threshold_follow_rfc8985_and_the_donor_cap` |
| Two-segment TLP threshold | `SRTT + SRTT/2 + delayed ACK`, capped by RTO | same test |
| TLP arm/fire | fire at the PTO, highest unacked segment, at most two probes | `tlp_arms_fires_twice_and_disarms_on_ack` |
| ACK/disarm | a full ACK disarms and resets the probe count | same test |
| DSACK reo_wnd growth | one `max(SRTT/4,1)` step per DSACK round, capped at SRTT | `dsack_grows_reo_wnd_once_per_round_and_ignores_tlp_dsacks` |
| TLP-caused DSACK | no reo_wnd growth | same test |
| Spurious undo | snapshot `cwnd`/`ssthresh`, restore once on DSACK or Eifel | `dsack_spurious_signal_restores_congestion_state_once` |
| Reneging | remove reneged SACK ranges, allow loss again | `reneging_clears_sacked_ranges_and_allows_loss_again` |
| Duplicate SACK | merge without changing the byte count | `duplicate_sack_merges_without_changing_the_byte_count` |
| Current primitives | `Tlp`, `Rack::on_dsack`, `SpuriousUndo` agree where they overlap | `model_matches_the_current_product_primitives` |

The randomized trace (`randomized_timers_and_sack_traces_are_deterministic_and_invariant`) runs
2,000 events for seeds `0x32000001..0x32000005`, interleaving sends, ACKs, SACKs, DSACKs, reneging,
TLP polls, recovery entry and Eifel signals. It asserts deterministic replay, `SND.UNA <= SND.MAX`,
`reo_wnd <= SRTT`, at most two probes, at most one undo per recovery, and sorted non-overlapping
SACK ranges.

## Donors

FreeBSD `sys/netinet/tcp_stacks/rack.c` at `133344405f532bd54ff3629a77d4c4e062f01fdb`
(BSD-2-Clause): `rack_calc_thresh_tlp`, `rack_timeout_tlp`, `rack_note_dsack`,
`rack_peer_reneges`, `rack_handle_might_revert`, `rack_exit_recovery`. RFC 8985 (RACK-TLP), RFC
6675 (SACK/reneging) and RFC 3522 (Eifel) are normative. Illumos has no RACK module; Linux
`net/ipv4/tcp_input.c`/`tcp_output.c` is read for outcome only. No donor expression is copied.

## Running

```text
cargo nextest run -p rack-tlp-model
```
