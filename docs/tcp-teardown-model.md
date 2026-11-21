<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP teardown host model (K316)

Status: host-only model, 2026-11-21. **OBSERVED** against the merged K267/K271 public
`raven_tcpip::Stack` API; no product source is edited here.

## Purpose

FIN/ACK, CLOSE-WAIT, LAST-ACK, FIN-WAIT-2, TIME-WAIT, 2MSL and RST are the teardown edges
that the RACK input rewrite will touch. `tests/tcp-teardown-model/` drives two `Stack`s over
the deterministic `raven-tcpip-sim` link and compares every observed state and timer deadline
with an RFC 9293 / FreeBSD teardown reference.

## Model

* `RefState` mirrors the eleven RFC 9293 states.
* `Event` covers local close/shutdown, bare ACK, FIN, FIN+ACK, RST and timer expiry.
* `transition` is the RFC 9293 Figure 6 teardown path, cross-checked against FreeBSD
  `sys/netinet/tcp_fsm.h` and `tcp_input.c`.
* `deadline` gives TIME-WAIT = 60 000 ms (2MSL) and FIN-WAIT-2 = 60 000 ms; other states have
  no teardown deadline.

## Coverage matrix

| Requirement | Test |
|---|---|
| local close then peer FIN reaches TIME-WAIT/2MSL | `half_close_reaches_time_wait_and_expires_after_2msl`; reference `local_close_then_peer_fin_reaches_time_wait` |
| peer FIN first: CLOSE-WAIT -> LAST-ACK -> CLOSED | reference `peer_fin_then_local_close_is_close_wait_last_ack_closed` |
| FIN-WAIT-2 timer | `fin_wait_two_has_its_own_deadline` |
| simultaneous close | `simultaneous_close_reaches_time_wait_on_both_sides`; reference `simultaneous_close_goes_through_closing` |
| retransmitted/duplicate FIN | `a_duplicate_fin_is_idempotent_on_the_peer` |
| 2MSL expiry | `half_close_reaches_time_wait_and_expires_after_2msl` |
| RST | `abort_sends_rst_and_closes_both_sides`; reference `rst_closes_every_state` |
| listener abort | `listener_abort_resets_a_half_open_child` |
| exact timer deadlines | `time_wait_and_fin_wait_two_have_the_configured_deadlines`, timer assertions in the stack tests |

The deterministic link is `LinkConfig::default()` (100 ms RTT, no loss/duplication) except
the duplicate-FIN case, which sets `duplicate_percent = 100.0` for the FIN.

## K267/K271 finding

`half_close_reaches_time_wait_and_expires_after_2msl` records a timer divergence:
`tcp_state.rs` sets `time_wait_deadline` on entering TIME-WAIT but does not clear
`fin_wait2_deadline`. The earlier FIN-WAIT-2 deadline therefore wins in
`next_poll_delay`, and the observed TIME-WAIT delay is shorter than the correct 60 000 ms.
The test `k267_fin_wait2_timer_is_still_armed_in_time_wait` pins the stale deadline and the
correct 2MSL value; the repair belongs to K267/K271. The state transitions themselves match
the reference.

## Donor and references

* FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`:
  `sys/netinet/tcp_input.c` (`tcp_do_segment`, FIN/ACK/RST arms),
  `sys/netinet/tcp_timer.c` (`tcp_timer_2msl`),
  `sys/netinet/tcp_usrreq.c` (`tcp_usrclosed`, `tcp_usr_shutdown`, `tcp_disconnect`),
  `sys/netinet/tcp_timewait.c` (`tcp_twstart`, `TT_2MSL`),
  `sys/netinet/tcp_fsm.h` state names. BSD-3-Clause; behaviour only, no code copied.
* RFC 9293 section 3.5 (close, half-close, simultaneous close, TIME-WAIT) and Figure 6.
* Illumos `usr/src/uts/common/inet/tcp/tcp_input.c`/`tcp_timers.c` for behaviour only (CDDL,
  no copy); Linux `net/ipv4/tcp_input.c`/`tcp_minisocks.c` for outcome only (GPL, no copy).

## Run

```sh
timeout 180 cargo nextest run -p tcp-teardown-model
timeout 420 make check-fast
```
