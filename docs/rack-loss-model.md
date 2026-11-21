<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK loss/reordering reference model (K319)

Host-only oracle for the future RACK port. The crate is
`tests/rack-loss-model` (`cargo nextest run -p rack-loss-model`); it is not
linked into the kernel, libc or `raven-tcpip`, and it changes no product code.

## What the model covers

* A send map of eight 1000-byte segments with send timestamps, SACK and ACK
  state, loss state and retransmit state.
* SACK blocks (segment-granular), duplicate SACK (a block below the cumulative
  ACK, or an explicit DSACK event) and the DSACK count.
* RACK reordering window: base `srtt + pkt_delay`; while reordering is recent add
  `srtt >> reorder_shift` (or `srtt / 4` at shift 0); with DSACK enabled add
  `dsack_count * (srtt / 4)`; cap at `2 * srtt` and at `rto_max`.
* RACK loss bound: an un-SACKed segment is lost when a later segment was SACKed
  and `now >= segment.sent_at + reo_wnd`.
* Fast-recovery entry on the first loss (or on the DSACK duplicate threshold) and
  exit when the cumulative ACK reaches the recovery point.
* Retransmit selection of the lowest lost, not-yet-retransmitted segment, one
  per ACK event.
* Pipe as RFC 6675-style `outstanding - sacked - lost + retransmitted`, with
  saturating arithmetic.

The production RACK port must match the recorded lost ranges, pipe, retransmit
list and recovery entry/exit counts for every trace.

## Trace format

`traces/rack-10k.tsv` has one row per seed `1..=10000`:

```text
seed  events  lost  pipe  retransmits  recovery_entries/exits  event_digest
```

* `lost` and `retransmits`: comma-separated `start-end` byte ranges, or `-`.
* `recovery_entries/exits`: two counts separated by `/`.
* `event_digest`: FNV-1a 64-bit digest of the canonical ACK/SACK/DSACK/RTT event
  list (`canonical_events` in `src/lib.rs`), so an input change is visible.
* Every fifth seed ends with a full ACK to exercise recovery exit; the other
  seeds leave a hole so lost ranges remain observable.

Current file:

* rows: 10,000 (`seed=1..10000`)
* rows with a final lost range: 7,948
* rows with a retransmit: 9,932
* recovery entries: 9,942; exits: 1,994
* SHA-256: `6d2b1df6d603dbd86546d697a90b9cbf515ad78a24d641e09d4399ed33fd939f`

Regenerate with:

```text
timeout 120 cargo run -q -p rack-loss-model -- \
  --generate tests/rack-loss-model/traces/rack-10k.tsv
```

## Donor reconciliation

The model is original clean-room test code; no donor code is copied. The exact
functions read for behaviour are FreeBSD `sys/netinet/tcp_stacks/rack.c` at
`133344405f532bd54ff3629a77d4c4e062f01fdb`:

| Topic | FreeBSD donor function | RFC | Raven model |
|---|---|---|---|
| reordering window | `rack_calc_thresh_rack` | RFC 8985 §6.2 | `reo_wnd` |
| loss marking | `rack_mark_lost`, `rack_log_sack_passed` | RFC 8985 §6.2 | `run` loss loop |
| recovery entry | `rack_cong_signal` (`CC_NDUPACK`), `rack_check_recovery_mode` | RFC 6675 §5 | `run` entries |
| recovery exit | `rack_post_recovery`, `rack_exit_recovery` | RFC 6675 §5 / RFC 6582 | `run` exits |
| SACK/DSACK intake | `rack_proc_sack_blk`, `rack_ack_received` | RFC 2018, RFC 2883 | `run` SACK/DSACK |
| retransmit choice | `rack_output` / `rack_mark_lost` consumer | RFC 8985 §6.2 | `run` retransmit selection |
| pipe | — (FreeBSD counts via `rc_prr_*`) | RFC 6675 §4 | `run` pipe |

Illumos `usr/src/uts/common/inet/tcp/tcp_output.c` and Linux
`net/ipv4/tcp_output.c` are behavioural cross-checks only; no code is copied.
No `PROVENANCE.toml` import is added because this is first-party test code.

## Limits

* Segment-granular SACK; partial-segment SACK splitting is not modelled.
* No TLP, RTO expiry, congestion-window growth, PRR pacing or ECN.
* No FIN/SYN; the model starts with data in flight.
* One retransmit selection per ACK event; a real stack may send more under PRR.

These limits are deliberate: the oracle covers the RACK loss/reordering state
machine and the wire events a future port must match, not the whole TCP stack.
