<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK output/timer/state matrix (K343)

Host-only matrix over the merged K330 output/timer/state module. The crate is
`tests/rack-output-matrix` (`cargo nextest run -p rack-output-matrix`); it is
test/evidence tooling and links no product code into the kernel.

It calls every public path of `RackOutput` with independent inputs and checks
the applicable contracts against the independent K320 TLP model and K327 vtable
model. K319 loss/reordering, K324 recovery/pipe and K326 BBR/pacing are
separate models; their algorithms are not duplicated here.

## Matrix

| Function | Rust | Test | Mutation | Status |
|---|---|---|---|---|
| `rack_output` | `RackOutput::rack_output` | `rack_output_sends_new_data` | window/MSS changed -> test fails | PASS |
| `tcp_rack_output` | `RackOutput::tcp_rack_output` | `tcp_rack_output_returns_candrop_on_closed` | return code changed -> test fails | PASS |
| `rack_get_persists_timer_val` | `RackOutput::rack_get_persists_timer_val` | `persist_timer_is_bounded` | clamp removed -> test fails | PASS |
| `rack_timer_start` | `RackOutput::rack_timer_start` | `timer_start_arms_and_sets_bits` | arm removed -> test fails | PASS |
| `rack_start_hpts_timer` | `RackOutput::rack_start_hpts_timer` | `hpts_timer_arms_a_slot` | arm removed -> test fails | PASS |
| `rack_timer_cancel` | `RackOutput::rack_timer_cancel` | `timer_cancel_clears_the_bits` | cancel removed -> test fails | PASS |
| `rack_timer_audit` | `RackOutput::rack_timer_audit` | `timer_audit_returns_due_bits` | audit ignores time -> test fails | PASS |
| `rack_remxt_tmr` | `RackOutput::rack_remxt_tmr` | `remxt_sets_override_and_backoff` | backoff removed -> test fails | PASS |
| `rack_stopall` | `RackOutput::rack_stopall` | `stopall_clears_all_timers` | stopall no-op -> test fails | PASS |
| `rack_stop_all_timers` | `RackOutput::rack_stop_all_timers` | `stop_all_timers_clears_bits` | stop no-op -> test fails | PASS |
| `rack_enter_persist` | `RackOutput::rack_enter_persist` | `enter_persist_arms_once` | second enter accepted -> test fails | PASS |
| `rack_exit_persist` | `RackOutput::rack_exit_persist` | `exit_persist_clears_once` | exit without enter accepted -> test fails | PASS |
| `rack_timeout_delack` | `RackOutput::rack_timeout_delack` | `delack_sets_acknow_once` | TF_ACKNOW not set -> test fails | PASS |
| `rack_timeout_rack` | `RackOutput::rack_timeout_rack` | `timeout_rack_requests_retransmit` | retransmit not requested -> test fails | PASS |
| `rack_timeout_tlp` | `RackOutput::rack_timeout_tlp` | `timeout_tlp_caps_at_two_probes` | third probe accepted -> test fails | PASS |
| `rack_timeout_rxt` | `RackOutput::rack_timeout_rxt` | `timeout_rxt_backs_off_and_closes` | close limit removed -> test fails | PASS |
| `rack_timeout_persist` | `RackOutput::rack_timeout_persist` | `timeout_persist_probes_and_rearms` | probe removed -> test fails | PASS |
| `rack_timeout_keepalive` | `RackOutput::rack_timeout_keepalive` | `keepalive_counts_and_closes` | probe cap removed -> test fails | PASS |
| `rack_process_timers` | `RackOutput::rack_process_timers` | `process_timers_dispatches_in_donor_order` | order changed -> test fails | PASS |
| `rack_set_state` | `RackOutput::rack_set_state` | `set_state_updates_timers` | invalid state accepted -> test fails | PASS |
| `rack_handoff_ok` | `RackOutput::rack_handoff_ok` | `handoff_rejects_fin_and_closed` | FIN accepted -> test fails | PASS |

## Model trace alignment

* **K320 TLP** (`rack-tlp-model`): `RackModel::new(default, 14600, u32::MAX)`,
  one segment sent, three polls at 1 ms intervals. The model fires exactly two
  probes; `RackOutput::rack_timeout_tlp` likewise produces two probe bytes and
  a zero third result. This is `trace_alignment_with_tlp_and_vtable_models`.
* **K327 vtable** (`rack-vtable-model`): an established transport with a FIN
  and outstanding data returns a handoff error; `RackOutput::rack_handoff_ok`
  rejects the same FIN state.
* **K319/K324/K326:** those models cover loss/reordering, recovery/pipe and
  BBR/pacing; the output module has no loss-map, pipe or BBR entry point, so
  there is no applicable trace row. This is recorded as a scope limit, not a
  skipped product path.

## K315 union row

The K343 slice contributes 21 rows. Run:

```text
cargo run -q -p rack-coverage -- \
  --donor /tmp/opencode/freebsd-rack.c \
  --matrix docs/rack-output-matrix.md \
  --manifest tests/rack-coverage/fixtures/rack-functions.txt
```

Expected output: 219 donor functions, 21 matrix rows, 198 missing (the other
K328-K336 groups), **0 duplicate, 0 unknown, 0 empty Rust/test/mutation, 0 not
PASS**. K338 owns the union register; this ticket supplies the K330 row set.

## Mutation and negative checks

* The matrix test expectation was mutated (`bytes_transmitted` 1460 -> 1461);
  `rack_output_sends_new_data` failed, then the assertion was restored and the
  full 23-test run passed.
* Negative paths exercised by the matrix: closed output returns
  `TCP_FUNC_OUTPUT_CANDROP`; a third TLP probe sends zero bytes; a FIN handoff
  is rejected; an invalid state is rejected; a second persist-exit is false.
