<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK pipe/lost test matrix (K345)

Status: host-only matrix, 2026-11-21. **OBSERVED** against the merged K333
`raven_tcpip::RackPipe` surface; no product source is edited by this ticket.

## Purpose

K333 merged the RACK pipe/lost/recovery group. `tests/rack-pipe-matrix/` runs that group as one
independent matrix: it calls every public `RackPipe` path, compares the shared arithmetic with the
K319/K320/K324 model oracles, regenerates the 10k trace files, and runs the K315 donor-function
row for the group. A missing function or mismatch is reported here as a finding for the K333
owner, never patched in product source.

## Public paths

Every public `RackPipe` method is called by `tests/public_paths.rs`.

| Path | Matrix checks |
|---|---|
| `RackPipe::new`, `Default::default` | empty sendmap, zero clock, initial RTO, donor defaults |
| `sendmap`, `push_sendmap`, `sendmap_mut` | transmission order, mutable flags/retransmit slot, unknown index |
| `now_us` | K313 `RackHpts` clock |
| `rack_grab_rtt` | RACK sample, then `t_srtt`, then `RACK_INITIAL_RTO` |
| `do_rack_compute_pipe`, `rack_compute_pipe` | `(snd_max - snd_una) - (sacked + considered_lost) + holes_rxt`, including the wrapping/negative path |
| `rack_calc_thresh_rack` | standard based, packet delay, recent reorder, reorder fade, DSACK growth, `2*srtt` and `rto_max` caps |
| `rack_calc_thresh_tlp` | `TLP_USE_ID`, `TLP_USE_TWO_ONE`, `TLP_USE_TWO_TWO`, threshold 0, `t_rxtcur`/`rto_max` caps and `rack_tlp_min` floor |
| `rack_mark_lost` | `RACK_SACK_PASSED` gate, expiry boundary, `RACK_WAS_LOST` skip, unknown index |
| `rack_mark_nolonger_lost` | flag clear, `considered_lost` decrement and zero clamp |
| `rack_check_recovery_mode` | first un-ACKed entry, ACKED skip, empty sendmap, early/equal/wrapped timestamp |
| `rack_check_recovery_mode_now` | K313 clock early and due paths |
| `rack_enter_recovery` | multiplicative decrease, PRR reset, ENTER_RECOVERY fields, ECN-CWR, no-PRR, already-in-recovery |
| `rack_exit_recovery` | both recovery flags cleared, unrelated flags kept |
| `rack_post_recovery` | RFC 6582 window, ssthresh floor, PRR addback with socket-buffer limit, no-PRR/no-addback, DSACK persist, RTO ssthresh restore |

## Trace oracles

Applicable oracles:

| Oracle | Comparison | Result |
|---|---|---|
| K319 `rack-loss-model` | checked-in `rack-10k.tsv` regenerated row for row; `reo_wnd` compared with `rack_calc_thresh_rack` on 1,000 seeds x 4 DSACK counts | PASS |
| K320 `rack-tlp-model` | `tlp_threshold` compared with `rack_calc_thresh_tlp` across SRTT/RTO values | PASS |
| K324 `rack-recovery-model` | checked-in `rack-recovery-10k.tsv` regenerated row for row; the model's final sacked/lost/retransmit state is fed to `rack_compute_pipe` for 200 seeds | PASS |
| K326 `rack-bbr-model` | not applicable: BBR/pacing does not exercise the pipe/lost arithmetic | N/A |
| K327 `rack-vtable-model` | not applicable: the vtable flags do not exercise the pipe/lost arithmetic | N/A |

The K319/K324 trace files are byte-for-byte regenerated, so a drift in either model or its checked-in
oracle fails the matrix rather than silently changing the comparison.

## K315 union row

The K315 checker is run over the nine K333 rows implemented in the merged module:

| Function | Rust | Test | Mutation | Status |
|---|---|---|---|---|
| `do_rack_compute_pipe` | `RackPipe::do_rack_compute_pipe` | `pipe_accounting_uses_the_donor_formula` | drop `considered_lost` -> 3950 vs 3750 | PASS |
| `rack_compute_pipe` | `RackPipe::rack_compute_pipe` | `pipe_accounting_uses_the_donor_formula` | same pipe mutation -> public wrapper fails | PASS |
| `rack_mark_lost` | `RackPipe::rack_mark_lost` | `rack_mark_lost_marks_sack_passed_entries_after_the_threshold` | drop the `RACK_SACK_PASSED` gate -> blocked entry marked | PASS |
| `rack_mark_nolonger_lost` | `RackPipe::rack_mark_nolonger_lost` | `rack_mark_nolonger_lost_clears_the_flag_and_decrements_the_count` | stop decrementing `considered_lost` -> 2500 vs 1500 | PASS |
| `rack_check_recovery_mode` | `RackPipe::rack_check_recovery_mode` | `rack_check_recovery_mode_finds_an_overdue_unacked_segment` | remove the overdue gate -> early call returns Some(0) | PASS |
| `rack_post_recovery` | `RackPipe::rack_post_recovery` | `rack_post_recovery_adds_back_prr_and_exits` | drop the PRR addback -> 8000 vs 9500 | PASS |
| `rack_exit_recovery` | `RackPipe::rack_exit_recovery` | `rack_exit_recovery_clears_both_recovery_flags` | clear only `TF_FASTRECOVERY` -> `TF_CONGRECOVERY` remains | PASS |
| `rack_calc_thresh_rack` | `RackPipe::rack_calc_thresh_rack` | `rack_thresh_rack_follows_standard_reorder_and_dsack_rules` | ignore `tmr_std_based` -> 1200 vs 1250 | PASS |
| `rack_calc_thresh_tlp` | `RackPipe::rack_calc_thresh_tlp` | `rack_thresh_tlp_uses_the_donor_threshold_and_caps` | remove the `rack_tlp_min` floor -> 75000 vs 100000 | PASS |

```text
donor: 219 function(s)
matrix: 9 row(s)
missing (210): the other K328-K336/K269 groups
duplicate: 0
unknown: 0
empty Rust: 0
empty test: 0
empty mutation: 0
not PASS: 0
```

The per-group `FAIL` is expected: K315 validates one group at a time and the remaining 210 donor
functions belong to the other groups. The K333 contribution itself has no duplicate, unknown,
empty or non-PASS row.

### Finding for the K333 owner

`docs/rack-coverage.md` assigns 11 donor functions to K333. The merged `rack_pipe.rs` exposes nine:

* `rack_enter_persist` (`rack.c:6278`)
* `rack_exit_persist` (`rack.c:6324`)

Neither symbol exists in `crates/raven-tcpip/src/rack_pipe.rs`, so there is no public path to call
and they are not claimed as PASS rows. This is a K333 finding, not a K345 product edit.

## Mutation and negative checks

* Product mutation: `do_rack_compute_pipe` was temporarily changed to ignore `considered_lost`;
  `pipe_accounting_uses_the_donor_formula` failed (3950 vs 3750) and the mutation was reverted.
* K315 negative checks: a dropped row is reported missing, an empty mutation cell is rejected, and a
  `FAIL` status is rejected.
* The public-path tests include the negative branches listed above (early/late loss, ACKED/empty
  recovery, no-PRR, no-addback, faded reorder, missing sendmap index, zero SRTT).

## Donor and references

* FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, `sys/netinet/tcp_stacks/rack.c`
  (BSD-2-Clause): `do_rack_compute_pipe`, `rack_compute_pipe`, `rack_calc_thresh_rack`,
  `rack_calc_thresh_tlp`, `rack_mark_lost`, `rack_mark_nolonger_lost`,
  `rack_check_recovery_mode`, `rack_post_recovery`, `rack_exit_recovery`, the
  `rack_cong_signal(CC_NDUPACK)`/`newreno_cong_signal` entry path, `rack_enter_persist` and
  `rack_exit_persist`; `sys/netinet/tcp_stacks/tcp_rack.h` for the flags and TLP modes.
  Behaviour only; no donor expression is copied.
* Illumos `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` (CDDL) and Linux
  `adc218676eef25575469234709c2d87185ca223a` (GPL): outcome/behaviour references only.
* K319/K320/K324 model crates and the K315 checker/manifest are first-party test code reused as
  oracles.

## Run

```sh
timeout 180 cargo nextest run -p rack-pipe-matrix
timeout 420 make check-fast

cargo run -q -p rack-coverage -- \
  --donor /tmp/opencode/freebsd-rack.c \
  --matrix docs/rack-pipe-matrix.md \
  --manifest tests/rack-coverage/fixtures/rack-functions.txt
```
