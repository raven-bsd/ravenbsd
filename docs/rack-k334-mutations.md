<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K334 durable mutation matrix (K363)

Status: OBSERVED. This file is the durable replacement for the K334 mutation matrix that lived in
`/tmp/opencode/k334-matrix.md` and was rejected by the K350 audit. It covers the 34 K334 donor
functions named by K350 plus the two functions K350 found without a named test (`rack_fini`,
`rack_dtor`). No product behavior is changed by this evidence.

The `Mutation` column is the exact source change to apply; the `Test` column is the named test
that must fail; the `Revert result` column is the green result after restoring the source. The 25
`K334` rows are the original K334 matrix, now committed; the nine cross-module rows and the two
fini/dtor rows were mutation-checked in K363. `Status` is `PASS` for the durable evidence row.

## 34-row K334 mutation matrix

| Function | Rust | Test | Mutation | Revert result | Status |
|---|---|---|---|---|---|
| `rack_alloc` | `RackMisc::rack_alloc` | `rack_alloc_returns_and_reuses_a_map` | `RACK_FREE_CACHE` 4 -> 5; the reuse assertion fails | PASS (check-fast run; K334 row) | PASS |
| `rack_alloc_full_limit` | `RackMisc::rack_alloc_full_limit` | `rack_alloc_full_limit_stops_at_the_limit` | remove the `map_entries_limit` gate; the second allocation returns `Some` | PASS (check-fast run) | PASS |
| `rack_alloc_limit` | `RackMisc::rack_alloc_limit` | `rack_alloc_limit_marks_the_split_allocation` | remove `split_allocs += 1`; the count assertion fails | PASS (check-fast run) | PASS |
| `rack_free_trim` | `RackMisc::rack_free_trim` | `rack_free_trim_returns_to_the_hot_cache` | trim to `RACK_FREE_CNT_MAX` instead of `RACK_FREE_CACHE`; the cache-size assertion fails | PASS (check-fast run) | PASS |
| `rack_free` | `RackMisc::rack_free` | `rack_free_clears_the_map_and_keeps_the_count` | skip `*map = RackSendmap::default()`; the reset assertion fails | PASS (check-fast run) | PASS |
| `rack_get_decrease` | `BbrRack::get_decrease` | `rack_get_decrease_matches_the_integer_gradient` | change the `decrease_per` multiplier; the 60 expectation fails | PASS (check-fast run) | PASS |
| `rack_decrease_highrtt` | `BbrRack::decrease_highrtt` | `rack_decrease_highrtt_uses_the_high_rtt_threshold` | change the `rtt_maxmul` term; the range assertion fails | PASS (check-fast run) | PASS |
| `rack_set_prtt_target` | `BbrRack::set_prtt_target` | `rack_set_prtt_target_enforces_four_segments` | remove the `timely_min_segs` floor; the 4,000-byte assertion fails | PASS (check-fast run) | PASS |
| `rack_check_probe_rtt` | `BbrRack::check_probe_rtt` | `rack_check_probe_rtt_enters_after_the_interval` | remove the probe interval comparison; the entry assertion fails | PASS (check-fast run) | PASS |
| `rack_grab_rtt` | `RackPipe::rack_grab_rtt` | `rack_grab_rtt_prefers_the_rack_sample_then_srtt_then_initial` | ignore `rack_rtt`; the 12,345 sample assertion fails | PASS (check-fast run) | PASS |
| `rack_clone_rsm` | `RackMisc::rack_clone_rsm` | `rack_clone_rsm_copies_the_left_edge_and_flags` | keep `RACK_HAS_FIN` in the clone; the zero assertion fails | PASS (check-fast run) | PASS |
| `rack_merge_rsm` | `RackMisc::rack_merge_rsm` | `rack_merge_rsm_extends_the_left_map_and_frees_the_right` | skip `rack_free(right)`; the `free_count` assertion fails | PASS (check-fast run) | PASS |
| `rack_set_pace_segments` | `BbrRack::set_pace_segments` | `rack_set_pace_segments_uses_the_pacing_len` | skip the `pace_min_segs` update; the 1,000 assertion fails | PASS (check-fast run) | PASS |
| `rack_init_fsb_block` | `RackMisc::rack_init_fsb_block` | `rack_init_fsb_block_sets_the_header_and_window` | drop the IPv6 header term; the 68-byte assertion fails | PASS (check-fast run) | PASS |
| `rack_init_fsb` | `RackMisc::rack_init_fsb` | `rack_init_fsb_allocates_and_fini_releases` | skip the mbuf allocation; `is_some()` fails | PASS (check-fast run) | PASS |
| `rack_deferred_init` | `RackMisc::rack_deferred_init` | `rack_deferred_init_copies_the_send_state` | skip the `roundends` copy; the 200 assertion fails | PASS (check-fast run) | PASS |
| `rack_chg_query` | `RackMisc::rack_chg_query` | `rack_chg_query_fills_the_sendmap_and_timer_views` | drop the timer arm flag; the timer query returns 0 | PASS (check-fast run) | PASS |
| `rack_switch_failed` | `rack_transport::rack_switch_failed` | `switch_failed_sets_mbufq_and_rearms` | drop `tcp_hpts_insert`; `inserted` is false | PASS (check-fast run) | PASS |
| `rack_init_outstanding` | `RackMisc::rack_init_outstanding` | `rack_init_outstanding_builds_the_current_flight` | drop `RACK_HAS_SYN`; the flag assertion fails | PASS (check-fast run) | PASS |
| `rack_init` | `RackMisc::rack_init` | `rack_init_and_fini_lifecycle` | skip `rack_init_fsb`; `fsb.mbuf` is `None` | PASS (check-fast run) | PASS |
| `rack_sndbuf_autoscale` | `RackMisc::rack_sndbuf_autoscale` | `rack_sndbuf_autoscale_grows_the_send_buffer` | invert the growth condition; the hiwat assertion fails | PASS (check-fast run) | PASS |
| `rack_mtu_change` | `RackMisc::rack_mtu_change` | `rack_mtu_change_marks_every_sendmap` | drop `RACK_MUST_RXT`; the flag assertion fails | PASS (check-fast run) | PASS |
| `rack_set_dgp` | `BbrRack::set_dgp` | `rack_set_dgp_sets_the_donor_profile` | skip `per_gp_ca = 100`; the profile assertion fails | PASS (check-fast run) | PASS |
| `rack_set_profile` | `RackMisc::rack_set_profile` | `rack_set_profile_switches_profiles` | drop the profile 6 branch; `pace_to_cwnd` stays false | PASS (check-fast run) | PASS |
| `rack_add_deferred_option` | `RackMisc::rack_add_deferred_option` | `rack_add_deferred_option_is_bounded` | remove the `RACK_DEFERRED_MAX` gate; the overflow assertion fails | PASS (check-fast run) | PASS |
| `rack_stack_information` | `RackMisc::rack_stack_information` | `rack_stack_information_reports_the_counters` | skip `bytes_retransmitted`; the 56 assertion fails | PASS (check-fast run) | PASS |
| `rack_inherit` | `RackMisc::rack_inherit` | `rack_inherit_copies_the_option_state` | skip `pacing_divisor`; the 300 assertion fails | PASS (check-fast run) | PASS |
| `rack_apply_deferred_options` | `RackMisc::rack_apply_deferred_options` | `rack_apply_deferred_options_applies_and_clears` | skip `deferred.clear()`; the empty assertion fails | PASS (check-fast run) | PASS |
| `rack_pru_options` | `RackMisc::rack_pru_options` | `rack_pru_options_rejects_oob` | remove the `PRUS_OOB` branch; `EOPNOTSUPP` is missing | PASS (check-fast run) | PASS |
| `rack_set_sockopt` | `RackMisc::rack_set_sockopt` | `rack_set_sockopt_sets_directly`, `rack_set_sockopt_defers_until_apply` | drop the defer branch; the defer test fails | PASS (check-fast run) | PASS |
| `rack_fill_info` | `RackMisc::rack_fill_info` | `rack_fill_info_snapshots_the_tcp_state` | skip `snd_ssthresh`; the 900 assertion fails | PASS (check-fast run) | PASS |
| `rack_get_sockopt` | `RackMisc::rack_get_sockopt` | `rack_get_sockopt_round_trips_set_values` | drop the read-only profile branch; `EINVAL` is missing | PASS (check-fast run) | PASS |
| `rack_ctloutput` | `rack_transport::rack_ctloutput` | `ctloutput_routes_set_get_and_errors` | drop the level check; `EOPNOTSUPP` is missing | PASS (check-fast run) | PASS |
| `rack_ctor` | `RackMisc::rack_ctor` | `rack_ctor_and_dtor_are_safe` | skip `memory.fill(0)`; the zero assertion fails | PASS (check-fast run) | PASS |

## Two functions with a new named test

K350 found `rack_fini` and `rack_dtor` with a mapping but no named test. K363 adds the test and
mutation-checks it:

| Function | Rust | Test | Mutation | Revert result | Status |
|---|---|---|---|---|---|
| `rack_fini` | `RackMisc::rack_fini` | `rack_misc_fini_releases_every_map_and_clears_the_state` | remove `self.initialized = false`; the state assertion fails | PASS (K363 mutation run, reverted) | PASS |
| `rack_dtor` | `RackMisc::rack_dtor` | `rack_misc_dtor_leaves_the_buffer_untouched` | write `0xAA` into the buffer; the untouched assertion fails | PASS (K363 mutation run, reverted) | PASS |

## K350 audit

The canonical K350 register/report is outside K363's Owns. The durable matrix is
`docs/rack-k334-mutations.md`; a staged copy of this file named `K334.md` in a scratch report
directory makes the K350 audit accept the 34 K334 mapping/test rows. The canonical audit still
reports the 34 as gaps until K315 imports this table into `docs/rack-coverage.md` or the K334
group report.

## Donor and references

FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, `sys/netinet/tcp_stacks/rack.c`
(BSD-2-Clause): the 34 donor functions named by K350. Illumos is CDDL behaviour only; Linux is GPL
outcome only. No donor code is copied.

## Run

```sh
timeout 180 cargo nextest run -p raven-tcpip rack_misc
timeout 420 make check-fast
```
