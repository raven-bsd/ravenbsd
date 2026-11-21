# TCP receive-window and zero-window model

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Host-only evidence tooling for the K265/K266 window path. The package is
`tests/tcp-window-model` (`cargo nextest run -p tcp-window-model`); it never edits product source.

## Model

`src/model.rs` is a clean-room reference for the receive window, persist probe, SWS and window
update rules. `src/harness.rs` drives the public `Stack`/`SocketId` API through the same cases and
hashes the delivered byte stream with FNV-1a.

| Case | Reference model | Real-stack test |
|---|---|---|
| Receive-window shrink | `free = rx_cap - queued`; 0 below the MSS/half-buffer floor | `receive_window_shrinks_to_zero_and_matches_model` |
| Zero-window probe | one forced byte when the window is closed and data is queued | `zero_window_probe_fires_and_the_stream_reopens` |
| Reopen/window update | newer `snd_wl1`/`snd_wl2` or a larger same-point window | `window_update_reopens_the_sender` |
| SWS | `len >= mss`, idle/NODELAY full queue, half the max window, or forced | `sws_withholds_a_partial_segment_until_the_window_fits_the_queue` |
| Persist/backoff | probe count and a saturating backoff shift | probe assertion in `zero_window_probe...` |
| Duplicate ACK | `ack == snd_una` leaves the window and `SND.UNA` unchanged | `duplicate_ack_does_not_move_the_window` |
| Stream integrity | every delivered byte enters the FNV-1a hash | all stream tests, plus `randomized_send_drain_stream_matches_the_reference_hash` |

The randomized test interleaves sends, application drains and polls, asserts that `RCV.NXT` moves
by exactly the delivered byte count, and compares the final stream hash with the original payload.

## Donors

FreeBSD `sys/netinet/tcp_output.c` (`tcp_output`, `tcp_setpersist`), `tcp_timer.c`
(`tcp_timer_persist`) and `tcp_input.c` (step 6 window update) at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-3-Clause). Illumos `tcp_output.c`/`tcp_timers.c`
is read for behaviour (CDDL) and Linux `net/ipv4/tcp_output.c`/`tcp_timer.c` for outcome (GPL) only.
No donor expression is copied.

## Running

```text
cargo nextest run -p tcp-window-model
```
