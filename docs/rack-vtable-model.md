<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK transport vtable model (K327)

Status: host-only independent model/evidence. **OBSERVED** by
`cargo nextest run -p rack-vtable-model`; no product source is changed.

## Scope

The future full RACK port installs the FreeBSD `tcp_function_block` callbacks in
`sys/netinet/tcp_var.h` and `sys/netinet/tcp_stacks/rack.c`. `tests/rack-vtable-model` is the
independent oracle for their **order, lifetimes, return values and named limits**, not a second
RACK core. K319/K320 own loss/reordering, TLP, DSACK and recovery; this model only traces the
vtable surface.

## Callback matrix

| Callback | Model contract |
|---|---|
| `tfb_tcp_fb_init` | allocate `t_fb_ptr`; defaults; double init `EINVAL` |
| `tfb_tcp_fb_fini` | clear HPTS/hw_tls/timers; double fini no-op; callback after fini `EINVAL` |
| `tfb_tcp_output` | retransmit when `r_must_retran`, send at `snd_max`, `snd_nxt = snd_max`, no send-buffer cut; pace insert only when sent |
| `tfb_tcp_do_segment` | drain `t_inqueue` first; RST closes, ACK advances/cuts `snd_una`, data records `t_rcvtime`, FIN enters CLOSE_WAIT |
| `tfb_do_segment_nounlock` | same state contract through the unlocked entry |
| `tfb_do_queued_segments` | drain; return 1 on a lost TCB, 0 otherwise |
| `tfb_tcp_ctloutput` | set/get PACE_MIN_SEG (mask 0xffff), PACE_MAX_SEG, PACING_DIVISOR (0 -> 250, below 50 -> 50), PACING_BETA set EINVAL, PACING_BETA_ECN newreno-only; unknown `EOPNOTSUPP` |
| `tfb_tcp_timer_stop_all` | `t_timers_stopped = 1`, remove HPTS, clear flags |
| `tfb_tcp_rexmit_tmr` | cancel RACK timer, set override/snapshot, `r_must_retran = 1`; clear the scoreboard only after `t_rxtshift > 2` |
| `tfb_tcp_handoff_ok` | CLOSED/LISTEN 0; SYN_SENT/SYN_RECEIVED `EAGAIN`; FIN with >1 outstanding `EAGAIN`; SACK permit or `rack_sack_not_required` 0; else `EINVAL` |
| `tfb_tcp_mtu_chg` | changed MSS retransmits, marks, snaps `snd_nxt = snd_max` |
| `tfb_pru_options` | reject only `PRUS_OOB` with `EOPNOTSUPP` |
| `tfb_hwtls_change` | set/clear `r_ctl.fsb.hw_tls` |
| `tfb_chg_query` | SENDMAP 0; TIMERS_UP 0/1 + flags/deadline; RACK_TIMES fills sacked/holes/lost; unknown `-EINVAL` |
| `tfb_switch_failed` | reapply `TF2_SUPPORTS_MBUFQ`/`TF2_MBUF_ACKCMP`; reinsert HPTS when not queued |
| `tfb_early_wake_check` | compare `now` against the pacing/timer deadline; false with no flags |
| `tfb_compute_pipe` | `(snd_max - snd_una) - (sacked + considered_lost) + holes_rxt` |
| `tfb_stack_info` | copy `t_sndbytes`/`t_snd_rxt_bytes` |
| `tfb_inherit` | same-stack only: copy the modelled options and `t_flags2` tail |

The model exposes the `tfb_refcnt` registration count separately; double register/deregister and
callback-before-init are explicit errors.

## Stable trace format

One line per callback, shared with the K319/K320 trace consumers:

```text
seq|callback|args|ret|fb|hw_tls|timers|hpts_flags|in_hpts|snd_una|snd_max|snd_nxt|sacked|lost|holes|rxtshift|must_retran
```

The state fields are decimal except `hpts_flags` (`0xNN`). `args` uses `key=value` pairs and never
contains `|`. The format is pinned by `all_nineteen_callbacks_have_a_trace_line` and the randomized
trace test checks every line has 17 fields.

## Randomized traces

Five seeds, 2,000 events each (10,000 total):
`0x32700001`, `0x32700002`, `0x32700003`, `0x32700004`, `0x32700005`.

Events are drawn from all 19 callbacks plus init/fini reinitialization. Each trace replays
identically, and after every event:

* `snd_una <= snd_max`;
* `hpts_flags` only contains the PACE timer/pacing bits;
* a live model has `pipe >= 0`;
* a finalized model has no HPTS/hw_tls and `compute_pipe == -EINVAL`.

## Donors

FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`:

* `sys/netinet/tcp_var.h` `struct tcp_function_block` :599-627, `TCP_FUNC_*` :533-534;
* `sys/netinet/tcp_hpts.h` PACE flags :80-93;
* `sys/netinet/tcp_stacks/rack.c` `__tcp_rack` :24075, `rack_init` :14467,
  `rack_fini` :14946, `rack_output` :19601, `rack_do_segment` :455/:16932,
  `rack_do_segment_nounlock` :16266, `rack_ctloutput` :24689, `rack_stopall` :8018,
  `rack_remxt_tmr` :7437, `rack_handoff_ok` :14907, `rack_mtu_change` :22539,
  `rack_pru_options` :24041, `rack_hw_tls_change` :24028, `rack_chg_query` :14163,
  `rack_switch_failed` :14258, `rack_wake_check` :24049, `rack_compute_pipe` :10668/:10677,
  `rack_stack_information` :22850, `rack_inherit` :23633.

Illumos has no RACK module; Linux is outcome-only. RFC 8985/6675 belong to K319/K320 and are not
duplicated here. No donor code is copied; no `PROVENANCE.toml` entry applies.

## Running

```sh
timeout 180 cargo nextest run -p rack-vtable-model
timeout 420 make check-fast
```

The evidence table, mutation and gate output are in `tickets/tim/reports/K327.md`.
