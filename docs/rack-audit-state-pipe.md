<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK state/pipe audit (K354)

Independent host-only audit of the merged K332 RACK input/state group and K333
RACK pipe/lost/recovery group. The audit reads the product sources with
`include_str!`, checks the donor-name inventory and named tests, compares every
declared `tcp_rack.h` / `tcp_var.h` / `tcp_fsm.h` constant, detects duplicate
declarations, and runs two exact donor vectors. It reports findings only; it
never edits product code.

## Run

```sh
timeout 120 cargo nextest run -p rack-audit-state-pipe
timeout 60 cargo run -q -p rack-audit-state-pipe
```

The checker covers 28 functions (18 K332 + 9 K333 + `rack_grab_rtt`), 28 named
tests, and 124 parsed constants from seven `rack_*.rs` files.

## Findings (38)

### Wrong donor constants (8)

| Name | File | Donor | Merged |
|---|---|---|---|
| `RACK_SACK_PASSED` | `rack_sack.rs` | `0x000010` | `0x2` |
| `RACK_WAS_SACKPASS` | `rack_sack.rs` | `0x000020` | `0x4` |
| `RACK_RWND_COLLAPSED` | `rack_sack.rs` | `0x000100` | `0x10` |
| `RACK_WAS_LOST` | `rack_sack.rs` | `0x400000` | `0x8` |
| `TCPS_CLOSING` | `rack_output.rs` | `7` | `8` |
| `TCPS_LAST_ACK` | `rack_output.rs` | `8` | `9` |
| `TCPS_FIN_WAIT_2` | `rack_output.rs` | `9` | `7` |
| `PACE_MAX_IP_BYTES` | `rack_retransmit.rs` | `65536` | `65535` |

The `rack_sack.rs` values are the donor names with a local sequential numbering;
they collide with `rack_misc.rs`/`rack_retransmit.rs` but have different values.
`rack_output.rs` renumbers `FIN_WAIT_2`/`CLOSING`/`LAST_ACK` away from
`tcp_fsm.h`. `rack_retransmit.rs` (K331) uses `65535` where
`rack_bbr_common.h` defines `65536`.

### Behavioural divergences (2)

| ID | File | Donor | Merged |
|---|---|---|---|
| `tlp-gap-index` | `rack_pipe.rs:rack_calc_thresh_tlp` | donor `inter_gap = rsm[idx] - prsm[nidx]`, threshold `6000` µs | merged subtracts `rsm[nidx] - prsm[idx]`, threshold `4000` µs |
| `fin-wait-without-fin` | `rack_state.rs:rack_do_fin_wait_1` | stays in `FIN_WAIT_1` unless `ourfinisacked` (`TF_SENTFIN` + full ACK) | moves to `FIN_WAIT_2` on `snd_una == snd_max` without a sent FIN |

Both vectors are executable in the crate (`tlp_gap_vector`, `fin_wait_vector`)
and are asserted by the live audit tests.

### Duplicate constants (25)

`TCPS_FIN_WAIT_1`, `TCPS_CLOSING`, `TCPS_LAST_ACK`, `TCPS_FIN_WAIT_2`
(`rack_state.rs` vs `rack_output.rs`); `TF_FASTRECOVERY`, `TF_CONGRECOVERY`
(`rack_pipe.rs` vs `rack_bbr.rs`); `RACK_ACKED`, `RACK_TO_REXT`, `RACK_OVERMAX`,
`RACK_SACK_PASSED`, `RACK_WAS_SACKPASS`, `RACK_HAS_FIN`, `RACK_TLP`,
`RACK_RWND_COLLAPSED`, `RACK_APP_LIMITED`, `RACK_WAS_ACKED`, `RACK_SENT_FP`,
`RACK_HAD_PUSH`, `RACK_MUST_RXT`, `RACK_PMTU_CHG`, `RACK_WAS_LOST`,
`RACK_NUM_OF_RETRANS`, `RACK_INITIAL_RTO` (two or three of
`rack_misc.rs`/`rack_retransmit.rs`/`rack_sack.rs`/`rack_bbr.rs`);
`RACK_RTO_MAX` (`rack_pipe.rs` vs `rack_retransmit.rs`); `PACE_MAX_IP_BYTES`
(`rack_retransmit.rs` vs `rack_bbr.rs`). The shared source of truth should be
`rack_misc.rs` for `RACK_*` and the K318 state constants.

### Other findings (3)

| ID | Kind | Evidence |
|---|---|---|
| `shared-test:do_rack_compute_pipe_subtracts_sacked_and_lost_and_adds_holes` | weak test | one test covers both `do_rack_compute_pipe` and `rack_compute_pipe`; no dedicated `rack_compute_pipe` test |
| `non-donor:rack_enter_recovery` | documented divergence | no `rack.c` symbol; models `rack_cong_signal(CC_NDUPACK)`; excluded from the K315 row as recorded in K333 |
| `duplicate-sendmap` | duplicate type | `rack_misc::RackSendmap` and `rack_retransmit::RackSendMap`/`Rsm` model the same `struct rack_sendmap` |

## Manual donor diff (for the group owner)

These are not machine-checked vectors but are exact donor/code differences found
while reading the group:

1. K332 `rack_do_established` omits the donor header-prediction fast path
   (`rack.c:13235-13257`, calls `rack_fastack`/`rack_do_fastnewdata`); the merged
   handler always calls `rack_process_data`.
2. K332 `rack_process_data` does not implement the donor window update, persist
   enter/exit and collapsed-window calls (`rack.c:12153-12200`); only the separate
   `rack_do_win_updates` helper handles a window change.
3. K332 `rack_handle_delayed_ack` arms on the timer flag alone; the donor
   `DELAY_ACK` gate (`rack.c:12101`, `TF_RXWIN0SENT`, `TF_DELACK`, `tlen <= MSS`
   and delayed-ACK enabled) is not modeled.
4. K333 `rack_post_recovery` reimplements the NewReno post-recovery arithmetic
   inline (`rack_pipe.rs`, `newreno_cc_post_recovery` comment). The donor calls
   `CC_ALGO(tp)->post_recovery` (`rack.c:5511`); K269's `cc::NewReno` is the CC
   owner, so this is duplicate CC logic.

## K315 union row

The group reports are valid per-group matrices:

```text
K332: donor 219, matrix 18, 0 duplicate/unknown/empty/not-PASS, 201 missing
K333: donor 219, matrix  9, 0 duplicate/unknown/empty/not-PASS, 210 missing
```

K332 + K333 are 27 distinct donor names, 0 overlap. The remaining 192 functions
belong to K269/K328-K336; the full union is assembled by the lead.

## Mutation / negative check

The audit itself was mutated by disabling the `WrongConstant` comparison; the
live conflicting-constant test failed (`missing audit finding
wrong:TCPS_FIN_WAIT_2`) and the source was restored. The crate also carries
negative fixtures for a missing function, a missing test, a wrong constant and a
duplicate constant.

## Donor

FreeBSD `sys/netinet/tcp_stacks/rack.c`, `sys/netinet/tcp_stacks/tcp_rack.h`,
`sys/netinet/tcp_var.h`, `sys/netinet/tcp_fsm.h`, and
`sys/netinet/tcp_stacks/rack_bbr_common.h` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause). Illumos and Linux
were used as behaviour/outcome references only; no donor code was copied.
