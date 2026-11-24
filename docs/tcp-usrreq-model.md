<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP usrreq/listen/accept/close host model (K321)

Status: host-only model, 2026-11-21. **OBSERVED** against the merged K271 public
`raven_tcpip::Stack` API; no product source is edited here.

## Purpose

K271 merged the usrreq/listen/close paths. `tests/tcp-usrreq-model/` drives the public Stack
API through listen/connect/accept/close/shutdown/RST and compares queue contents, errors and
socket states with an independent listener reference, including backlog overflow and listener
abort.

## Model

* `RefListener` is a FIFO completed-connection queue.
* `completed_capacity(backlog)` follows FreeBSD `sys/kern/uipc_socket.c`: the backlog is clamped
  to `somaxconn`, and `solisten_clone` refuses a child beyond `3 * backlog / 2 + 1`.
* `push` refuses when full, `accept` pops the oldest, and `close` abandons the queue and marks
  the listener closed.

## Coverage matrix

| Requirement | Test |
|---|---|
| listen errors: port 0, duplicate, closed accept, listener shutdown | `listen_rejects_bad_ports_and_closed_accept` |
| FIFO accept queue and backlog capacity | `accept_queue_is_fifo_and_bounded_by_the_backlog` |
| real-handshake backlog overflow | `backlog_overflow_with_real_handshakes_drops_the_third_child` |
| half-open overflow with SYN cookies off | `half_open_overflow_is_counted_when_syn_cookies_are_off` |
| listener abort of a completed-but-unaccepted child | `listener_abort_resets_a_completed_but_unaccepted_child` |
| shutdown/close errors and RST | `shutdown_close_and_rst_surface_the_expected_errors` |
| 10k randomized listener operations | `ten_thousand_random_listener_operations_match_the_reference` |

Random seed: `0x32105a4351554555`. The 10k run performs 3816 queued adds, 2168 accepts and 526
listener close/reopen cycles; it compares every accept result with the reference FIFO and drains
both queues every 100 operations.

## K271 findings

1. **Backlog overflow counter.** `AcceptQueue::push` enforces the backlog-derived capacity and
   records the drop in its own `stats`, but the global `Counters::accept_queue_drops` only
   fires when `pending.len() >= tcp.accept_queue_limit` (128). With `backlog=1` the third child
   is correctly not accepted, but the global counter stays zero. The test asserts the observable
   bound (two accepted children) and records the counter mismatch for K271.
2. **Listener close representation.** `tcp_state(listener)` continues to report `Listen` after
   `close` marks the record closed; the test observes the child reset instead. This is a
   state-reporting nuance, not an accept-queue failure.

## Donor and references

* FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`:
  `sys/kern/uipc_socket.c` (`solisten_proto`, `solisten_clone`, `sonewconn`, `soabort`),
  `sys/netinet/tcp_usrreq.c` (`tcp_usr_listen`, `tcp_usr_accept`, `tcp_usr_close`,
  `tcp_usr_shutdown`, `tcp_usr_abort`), `sys/netinet/tcp_input.c` (SYN/ACK promotion).
  BSD-3-Clause; behaviour only, no code copied.
* RFC 9293 section 3.8 (listen/backlog/accept).
* Illumos `usr/src/uts/common/inet/tcp/tcp_input.c` (behaviour only, CDDL); Linux
  `net/ipv4/tcp_minisocks.c` (outcome only, GPL).

## Run

```sh
timeout 180 cargo nextest run -p tcp-usrreq-model
timeout 420 make check-fast
```
