<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK canonical flag/constant conformance matrix

Host-only matrix crate `tests/rack-flags` (K360). It asserts every canonical RACK constant exported
from `crates/raven-tcpip/src/rack_types.rs` against the pinned FreeBSD
`sys/netinet/tcp_stacks/tcp_rack.h`/`rack.c`/`rack_bbr_common.h`/`tcp_var.h` values at
`133344405f532bd54ff3629a77d4c4e062f01fdb`, and exercises the sendmap/flag transitions in the
merged `rack_*` modules. It never edits product source.

## Running

```text
timeout 180 cargo nextest run -p rack-flags
timeout 420 make check-fast
```

`fixtures/tcp_rack_flags.txt` is the donor `name value type` manifest. `src/lib.rs` maps every name
to the exported `raven_tcpip` constant and compares value and type; a duplicate, unknown, wrong
value or wrong type is a failure.

## Canonical constants

| Name | Donor value | Type |
|---|---:|---|
| `RACK_ACKED` | `0x000001` | `u32` |
| `RACK_TO_REXT` | `0x000002` | `u32` |
| `RACK_DEFERRED` | `0x000004` | `u32` |
| `RACK_OVERMAX` | `0x000008` | `u32` |
| `RACK_SACK_PASSED` | `0x000010` | `u32` |
| `RACK_WAS_SACKPASS` | `0x000020` | `u32` |
| `RACK_HAS_FIN` | `0x000040` | `u32` |
| `RACK_TLP` | `0x000080` | `u32` |
| `RACK_RWND_COLLAPSED` | `0x000100` | `u32` |
| `RACK_APP_LIMITED` | `0x000200` | `u32` |
| `RACK_WAS_ACKED` | `0x000400` | `u32` |
| `RACK_HAS_SYN` | `0x000800` | `u32` |
| `RACK_SENT_W_DSACK` | `0x001000` | `u32` |
| `RACK_SENT_SP` | `0x002000` | `u32` |
| `RACK_SENT_FP` | `0x004000` | `u32` |
| `RACK_HAD_PUSH` | `0x008000` | `u32` |
| `RACK_MUST_RXT` | `0x010000` | `u32` |
| `RACK_IN_GP_WIN` | `0x020000` | `u32` |
| `RACK_SHUFFLED` | `0x040000` | `u32` |
| `RACK_MERGED` | `0x080000` | `u32` |
| `RACK_PMTU_CHG` | `0x100000` | `u32` |
| `RACK_STRADDLE` | `0x200000` | `u32` |
| `RACK_WAS_LOST` | `0x400000` | `u32` |
| `RACK_IS_PCM` | `0x800000` | `u32` |
| `RACK_NUM_OF_RETRANS` | `3` | `usize` |
| `RACK_INITIAL_RTO` | `1000000` | `u32` |
| `RACK_RTO_MIN` | `30000` | `u32` |
| `RACK_RTO_MAX` | `4000000` | `u32` |
| `RACK_TLP_MIN` | `10000` | `u32` |
| `RACK_DELAYED_ACK_TIME` | `40000` | `u32` |
| `RACK_PRR_ADDBACKMAX` | `2` | `u32` |
| `RACK_REORDER_FADE_DEFAULT` | `60000000` | `u32` |
| `PACE_MAX_IP_BYTES` | `65536` | `u32` |
| `TF_FASTRECOVERY` | `0x00100000` | `u32` |
| `TF_WASFRECOVERY` | `0x00200000` | `u32` |
| `TF_CONGRECOVERY` | `0x20000000` | `u32` |
| `TF_WASCRECOVERY` | `0x40000000` | `u32` |

## Sendmap transitions

| Transition | Public path | Test |
|---|---|---|
| flags are unique and clearable | `RackSendmap.flags` | `every_flag_bit_is_unique_and_round_trips` |
| `ACKED` set, idempotent | `RackSendMap::mark_acked` | `sendmap_mark_acked_sets_the_acked_flag` |
| `SACK_PASSED`→`WAS_SACKPASS`, clear `ACKED`/`WAS_LOST`/`MUST_RXT`/`RWND_COLLAPSED` | `RackRetransmit::rack_update_rsm` | `retransmit_update_rsm_transitions_all_flags` |
| `OVERMAX` after the retransmit cap | `RackRetransmit::rack_update_rsm` | `retransmit_overmax_and_edge_split` |
| `HAS_FIN`/`HAD_PUSH` move to the split tail; `HAS_SYN` stays left | `RackRetransmit::rack_update_entry` | same |
| `TLP` set/clear for a TLP probe | `RackRetransmit::rack_fast_rsm_output` | `retransmit_tlp_flag_toggles` |
| `HAS_SYN`/`HAS_FIN`/`HAD_PUSH` cleared on clone; `HAS_FIN`/`TLP`/`RWND_COLLAPSED`/`APP_LIMITED` carried on merge | `RackMisc::rack_clone_rsm`, `rack_merge_rsm` | `misc_clone_and_merge_cover_the_edge_flags` |
| `MUST_RXT` + `PMTU_CHG` on MTU change | `RackMisc::rack_mtu_change` | `misc_mtu_change_sets_must_rxt_and_pmtu` |
| `ACKED`, `WAS_LOST` on SACK processing | `RackSack::rack_proc_sack_blk`, `rack_log_sack_passed` | `sack_transitions_acked_sack_passed_was_lost` |
| `WAS_LOST` set/clear | `RackPipe::rack_mark_lost`, `rack_mark_nolonger_lost` | `pipe_marks_and_clears_was_lost` |

Flags with no setter in the merged K328-K336 groups (`TO_REXT`, `DEFERRED`, `WAS_ACKED`,
`SENT_W_DSACK`, `SENT_SP`, `SENT_FP`, `IN_GP_WIN`, `SHUFFLED`, `MERGED`, `STRADDLE`, `IS_PCM`) are
still covered by the value/type matrix and the unique/round-trip bit test; the matrix does not
invent a transition for them.
