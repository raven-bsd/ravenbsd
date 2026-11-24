<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK misc/vtable test matrix

Status: K346, 2026-11-21. Evidence tags: **OBSERVED** (measured), **PROPOSED**,
**UNVERIFIED**.

`tests/rack-misc-matrix` is a host-only matrix over the merged RACK misc (`rack_misc.rs`) and
vtable (`rack_vtable.rs`) modules. It calls every public path, checks the durable values against
the independent K319/K320/K324/K326/K327 model traces where applicable, and supplies the K315 union
row for the K334 misc group. No product source is edited.

## 1. Matrix

| Test | Public paths exercised |
|---|---|
| `misc_sendmap_and_lifecycle_matrix` | `rack_alloc`, `rack_alloc_full_limit`, `rack_alloc_limit`, `rack_free`, `rack_free_trim`, `rack_clone_rsm`, `rack_merge_rsm`, `rack_init_fsb_block`, `rack_init_fsb`, `rack_deferred_init`, `rack_init_retransmit_value`, `rack_init_outstanding`, `rack_init`, `rack_fini`, `rack_mtu_change`, `rack_sndbuf_autoscale` |
| `misc_options_query_and_trace_matrix` | `rack_set_profile`, `rack_add_deferred_option`, `rack_apply_deferred_options`, `rack_set_sockopt`, `rack_get_sockopt`, `rack_process_option`, `rack_inherit`, `rack_fill_info`, `rack_stack_information`, `rack_chg_query`, `rack_ctor`, `rack_dtor`, `rack_pru_options` |
| `vtable_matrix_calls_every_callback` | all 19 `RackVtable` callbacks: output, segment, queued segment, ctloutput, init/fini, stop-all, rexmit, handoff, MTU, PRU, HW-TLS, query, switch-failed, early-wake, pipe, stack-info, inherit |
| `vtable_trace_values_match_the_reference_models` | function-block metadata, pipe calculation, RACK query times, profile/sockopt defaults |

The 28 K334 donor functions are exactly the rows in `fixtures/k334-matrix.md`.
`rack_process_option` is exercised as a public path but its donor row remains K332's private
dispatcher, matching K334's assignment.

## 2. Trace comparisons

The independent model traces are the K319 loss/reference model, K320 TLP/DSACK model, K324
recovery/pipe model, K326 BBR/pacing model and K327 vtable model. The matrix pins the applicable
shared values:

* function-block metadata: name `rack`, refcount 1, output-candrop/default-ok flags;
* default pacing divisor 250 and HPTS slot 10 µs;
* RFC 6675 pipe: `snd_max - snd_una - sacked - considered_lost + holes_rxt` = 830 for
  `(1000, 100, 50, 25, 5)`;
* `TCP_QUERY_RACK_TIMES` returns `rack_rtt`/`rack_srtt_measured` unchanged;
* profile 1 sets DGP, always-pace, CMP-ACK and MBUFQ;
* the 19-callback vtable surface is complete and callable in one lifecycle.

## 3. Commands

```sh
timeout 180 cargo nextest run -p rack-misc-matrix
timeout 420 make check-fast
```

K315 union row:

```sh
timeout 120 cargo run -q -p rack-coverage -- \
    --donor /tmp/opencode/fbsd-rack.c \
    --matrix tests/rack-misc-matrix/fixtures/k334-matrix.md \
    --manifest tests/rack-coverage/fixtures/rack-functions.txt
```

Observed result:

```text
donor: 219 function(s)
matrix: 28 row(s)
manifest: 219 entries match
missing (191): rack_get_lt_bw, ... rack_ctor
result: FAIL (191 missing, 0 duplicate, 0 unknown, 0 empty Rust, 0 empty test,
              0 empty mutation, 0 not PASS)
```

The 191 missing are the other RACK group tickets; for this group the union is clean. Deleting one
row makes the checker report `192 missing`, which is the negative check.

## 4. Mutation/negative checks

| Check | Result |
|---|---|
| `tfb_compute_pipe` expectation 830 changed to 831 | `vtable_matrix_calls_every_callback` FAIL (exit 100), then restored green |
| `rack_alloc` row deleted from the K315 matrix | checker `192 missing` (exit 1) |
| full matrix | 4 tests pass |

## 5. Donor reconciliation

Test-only: the expectations come from FreeBSD `sys/netinet/tcp_stacks/rack.c` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause) as implemented by K334/K318 and the
independent K319/K320/K324/K326/K327 models. No donor code is copied and no product file changes.

## 6. Gaps

* K315 still exits 1 overall until the other 191 donor functions are merged; this group's 28 rows
  have no duplicate, unknown, empty or non-PASS row.
* The matrix is a public-path/behavioral check, not a line-for-line donor replay; it pins the
  durable outputs the models expose and every public callback is callable.
* `rack_process_option` is a public dispatcher but its donor row belongs to K332; it is exercised
  here without taking ownership of the matrix row.
