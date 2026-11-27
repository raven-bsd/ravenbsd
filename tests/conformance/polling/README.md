<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# Polling conformance (S173)

`polling-conformance.rs` asserts the POSIX/FreeBSD polling contract on the release image. The
shared runner in `../harness/runner.py` builds the probe, overlays it as a one-shot service, boots
the image and maps the rows to an exit status.

## Rows

| Row | Contract asserted |
|---|---|
| `pipe-readable` | `poll(0)` reports no `POLLIN` on an empty pipe and exactly `POLLIN` after one byte |
| `pipe-hup` | after the writer closes and the data is drained, `poll(0)` reports `POLLHUP` |
| `select-readable` | `select` with a zero timeout reports the same byte on the read set |
| `timeout` | `poll(50)` on an idle pipe returns 0 after at least the requested milliseconds |
| `eintr` | a signal with a handler interrupts a bounded `poll` with `EINTR` |
| `socket-error` | a refused non-blocking connect reports `POLLERR`; `SO_ERROR` returns `ECONNREFUSED` once then 0 |
| `eventfd` | not readable before a write and readable after it; level-vs-edge persistence is post-0.1.0 |
| `timerfd` | not readable before expiry, not readable when armed, readable after the expiry |

## Controlled mutation

`hup-not-reported` -- the probe expects a closed writer not to report `POLLHUP`, which is what a
poll that drops hangup would believe. The correct product reports it, so `pipe-hup` turns red.

## Donor map

FreeBSD donor `133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause), Illumos
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` (CDDL, behaviour only) and Linux
`adc218676eef25575469234709c2d87185ca223a` (GPL, outcome only).

| Topic | Illumos finding | BSD file + licence | Linux finding | Decision |
|---|---|---|---|---|
| `poll` scan and `POLLNVAL` | `pollhead`/`polllock` level model | FreeBSD `sys/kern/sys_generic.c` `kern_poll_kfds`, `pollscan`, `pollout` (BSD-2) | `fs/select.c` `do_poll` outcome only | test only: per-fd `POLLNVAL`, `POLLIN` only at a ready level |
| `select` readiness | same pollhead model | FreeBSD `sys/kern/sys_generic.c` `kern_select`, `selscan`, `selrescan` (BSD-2) | `fs/select.c` `core_sys_select` outcome only | test only: a zero timeout returns the current read set |
| `POLLHUP` at writer close | pipe end state, hangup is reported with the rest | FreeBSD `sys/kern/sys_generic.c` `pollout` and `sys/kern/sys_pipe.c` `pipe_poll` (BSD-2) | outcome only | test only: `POLLHUP` after close and drain |
| `EINTR` | signal interrupts the wait; the raw wait is not restarted | FreeBSD `sys/kern/sys_generic.c` `kern_poll_kfds` (`poll is not restarted after signals`) (BSD-2) | outcome only | test only: handler-bearing signal gives `EINTR` |
| socket `POLLERR`/`SO_ERROR` | `so_error` reported once and cleared | FreeBSD `sys/kern/uipc_socket.c` `soo_poll`/`soreceive` error path (BSD-2) | outcome only | test only: `POLLERR` and one `SO_ERROR` read |
| eventfd readable level | `portfs`/event behaviour only | FreeBSD `sys/kern/sys_eventfd.c` `eventfd_poll` (`efd_count > 0`), `eventfd_write` (BSD-2) | `fs/eventfd.c` outcome only | test only: readable only with a non-zero counter, until read |
| timerfd readable level | timer expiry behaviour only | FreeBSD `sys/kern/sys_timerfd.c` `timerfd_poll` (`tfd_expired`), `timerfd_expire` (BSD-2) | `fs/timerfd.c` outcome only | test only: readable only after expiry |

No donor expression is copied; the probe asserts observable `revents` bits, errno values and
timeout results.

## Run

```sh
python3 tests/conformance/harness/runner.py --topic tests/conformance/polling \
    --service polling-conformance
python3 tests/conformance/harness/runner.py --topic tests/conformance/polling \
    --service polling-conformance --phase mutation --mutation hup-not-reported
```
