<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# CUBIC/NewReno/RACK model audit (K359)

Status: host-only audit, 2026-11-21. **OBSERVED** against the K341
`tests/cc-model` traces and `origin/master` `95cc0c6d`; no product source is edited.

## Purpose

K341 adds the 10k CUBIC/NewReno/RACK traces. `tests/cc-model-audit/` checks that the trace file
is deterministic and internally consistent, recomputes the key values from the FreeBSD donor
formulas, and checks the K269 CUBIC/RACK mapping rows against the merged tree. A mismatch or a
missing row is a finding for the K341/K269 owner, never a product edit in this ticket.

## Trace audit

`tests/cc-model-audit/tests/trace_audit.rs`:

| Check | Result |
|---|---|
| `cc-10k.tsv` regenerated from `cc_model::{Scenario, run, trace_row}` | PASS: byte for byte |
| 10,000 rows parsed with 9 columns and the 2-line header | PASS |
| every row equals a fresh model run and a fresh digest fold | PASS |
| class coverage by `seed % 3` (NewReno / CUBIC / RACK) | PASS: 3,334 / 3,333 / 3,333 |
| range checks: events 8..16, windows > 0, lost <= events, retransmits <= lost, pipe >= MSS | PASS |

The trace is deterministic and consistent with the model. The model itself is not donor-equivalent;
the findings below are about the formulas the trace records, not about trace-file drift.

## Donor formula diff

The donor oracle is in `tests/cc-model-audit/src/lib.rs` and is transcribed from the FreeBSD
`cc.c`, `cc_newreno.c`, `cc_cubic.c`, `cc_cubic.h` and `rack.c` functions listed in the report.
The audit uses 10,000-byte windows, 1,000-byte MSS and the K341 scenario generator.

| ID | Area | K341 model | FreeBSD/RFC donor | Difference |
|---|---|---|---|---|
| K359-F01 | NewReno loss/recovery | `ssthresh = cwnd/2`, `cwnd = ssthresh` at the loss | `newreno_cc_cwnd_on_multiplicative_decrease` returns `max(cwnd*50/(100*mss),2)*mss` (MSS-rounded); `newreno_cc_post_recovery` uses the pipe floor | 10,000 -> model cwnd 5,000; donor ssthresh 5,000 and post-recovery cwnd 2,000. 10,500 -> model 5,250; donor 5,000 |
| K359-F02 | NewReno CA/ABC | always `mss^2/cwnd` (50 bytes at cwnd 20,000); slow start caps at `2*ssthresh` | RFC 3465 ABC default: 0 or one MSS per ACK via `CCF_ABC_SENTAWND`; `newreno_cc_cwnd_in_cong_avoid` has no `2*ssthresh` cap | the model has no per-ACK ABC latch, and the `2*ssthresh` cap is not donor behavior |
| K359-F03 | CUBIC loss | first loss: `W_max = cwnd = 10,000`, `ssthresh = cwnd*179/256 = 6,992`, `cwnd = ssthresh` | `W_max` starts at `UINT_MAX`; fast convergence gives `W_max = 10,000*217/256 = 8,476`; `ssthresh = pipe*179/256 = 5,593`; `cwnd` is reduced only in `cubic_post_recovery` (2,000) | the model uses cwnd instead of pipe, skips the first fast convergence and reduces cwnd too early |
| K359-F04 | CUBIC Reno-friendly | `0.7*W_max + t` (t in seconds) = 8.1 segments at W_max 10, t 1.1 | RFC 9438 `W_est = W_max*beta + (3*(1-beta)/(1+beta))*(t/RTT)` = 12.8235 segments; donor `tf_cwnd` is the NewReno CA cwnd | the model drops the `1/RTT` scaling and uses a different alpha |
| K359-F05 | CUBIC target | no target cap; bytes and segments mixed | `cubic_ack_received` caps the target at `1.5*cwnd` (5 segments -> 3 at cwnd 2) | the model can grow by more than 1.5x per ACK |
| K359-F06 | RACK loss | no cwnd/ssthresh change on a RACK loss | `rack_cong_signal(CC_NDUPACK)` calls the NewReno/CUBIC multiplicative decrease | model final cwnd 20,000 vs donor 2,000 (ssthresh 5,000) |
| K359-F07 | RACK pipe | `outstanding - lost_bytes`, floor one MSS; no `rc_sacked`/`rc_holes_rxt` | `do_rack_compute_pipe = (snd_max-snd_una)-(sacked+considered_lost)+holes_rxt`, no floor | sacked 2,000/lost 1,000 -> donor 5,000 vs model 7,000; empty flight -> donor 0 vs model 1,000 |
| K359-F08 | RACK retransmit | one retransmit per loss event, possibly the same range twice | `rack_mark_lost` skips `RACK_WAS_LOST`; `rack_find_lowest_rsm` selects the lowest not-yet-retransmitted range | two loss events at the same `snd_una` select `(1000,2000)` twice in the model |

K359-F01..F08 are K341/K269 implementation findings. The trace file is not edited; the audit
records the donor values and the model values so the owner can fix the model or explicitly record
a divergence.

## K269 row audit

`tests/cc-model-audit/tests/k269_audit.rs` checks the K269 report's rows against `origin/master`
`95cc0c6d`:

| ID | Claimed row | Merged tree | Finding |
|---|---|---|---|
| K359-F09 | K269-R01..R08: CUBIC `Cubic::ack_received`, `congestion`, `post_recovery`, `record_rtt`, `rttsample`, `get_ssthresh`, `after_idle`, `newround`, `cubic_k`, `cubic_cwnd`, `tf_cwnd`; tests in `cc_ut.rs` | `cc.rs` still has the RFC 8312 `loss_event`/`advance`/`rto_event`/`w_last_max`; `cc_ut.rs` and all 18 named tests are absent | the CUBIC rows are unmerged; the K269 report itself is `WORKING` and says the CUBIC port is blocked |
| K359-F10 | K269-R09 and the RACK mapping table: `Rack::ack_received`, `process_ack`, `update_entry`, `update_rsm`, `post_recovery`, `cong_signal`, `update_rtt`, `note_dsack`, `process_to_cumack`, `reorder_threshold_us`, `process_sack_block`, `oldest_unacked_index`, `highest_unacked`; 22 `rack_core_*` tests | merged `recovery.rs` has the smaller `Rack::on_sent`/`is_lost`/`on_dsack`/`on_ack`/TLP detector; none of the claimed methods/tests exist | the RACK rows are unmerged |
| K359-F11 | `docs/rack-coverage.md` marks `rack_cong_signal` and `rack_update_prr` K269 PASS with six named tests | neither `Rack::cong_signal` nor `Rack::rack_update_prr` exists in `crates/raven-tcpip`; all six tests are absent | the canonical matrix has two stale PASS rows; K269 is not full RACK acceptance |

The K269 report's rows are a snapshot of the unmerged `dave/K269` branch, not of
`origin/master`. Missing and weak rows are findings for K269; K359 does not edit the report or
the product tree.

## Mutation and negative checks

* CUBIC donor oracle mutated `179/256 -> 178/256`: `cubic_loss_uses_cwnd...` failed with
  `left 5562, right 5593`; reverted to PASS.
* Trace digest mutated by dropping the `event.at` fold: `every_trace_row_matches...` failed at
  seed 1 with `left 10517607816160144873, right 15936381075758875686`; reverted to PASS.
* The K269 audit asserts the absence of both the claimed symbols and the claimed test names, and
  the presence of the smaller merged RACK detector tests, so a future merge changes the finding
  set rather than silently passing.

## Donor and references

* FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause):
  `sys/netinet/cc/cc.c` `newreno_cc_post_recovery`, `newreno_cc_after_idle`,
  `newreno_cc_cwnd_on_multiplicative_decrease`, `newreno_cc_cong_signal`,
  `newreno_cc_cwnd_in_cong_avoid`, `newreno_cc_cwnd_in_slow_start`,
  `newreno_cc_ack_received`; `sys/netinet/cc/cc_newreno.c` `newreno_ack_received`,
  `newreno_cong_signal`, `newreno_after_idle`; `sys/netinet/cc/cc_cubic.c`
  `cubic_ack_received`, `cubic_does_slow_start`, `cubic_after_idle`, `cubic_cong_signal`,
  `cubic_post_recovery`, `cubic_record_rtt`, `cubic_get_ssthresh`, `cubic_rttsample`,
  `cubic_newround`; `sys/netinet/cc/cc_cubic.h` `cubic_k`, `cubic_cwnd`, `tf_cwnd` and the
  `CUBIC_BETA`/`CUBIC_FC_FACTOR`/`CUBIC_C_FACTOR` constants; `sys/netinet/tcp_stacks/rack.c`
  `do_rack_compute_pipe`, `rack_cong_signal`, `rack_post_recovery`, `rack_mark_lost`.
  Behaviour/formula comparison only; no donor expression is copied into product code.
* Illumos `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`,
  `usr/src/uts/common/inet/tcp/tcp_input.c` (`cc_ack_received`, `cc_cong_signal`): CDDL,
  behaviour only.
* Linux `adc218676eef25575469234709c2d87185ca223a`: the local sparse checkout has no
  `net/ipv4/tcp_cubic.c`, so no Linux CUBIC source was read; the audit uses RFC 9438/8985 and the
  FreeBSD donor.
* RFC 9438 (CUBIC) and RFC 8985 (RACK-TLP).

## Run

```sh
timeout 180 cargo nextest run -p cc-model-audit
timeout 420 make check-fast
```
