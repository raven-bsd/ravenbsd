<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# kqueue conformance (S743)

`kqueue-conformance.rs` asserts the S743 declared kqueue subset on the release image. The shared
runner in `../harness/runner.py` builds the probe, overlays it as a one-shot service, boots the
image and maps the rows to an exit status.

## Rows

| Row | Contract asserted |
|---|---|
| `layout` | `sizeof(struct kevent) == 64` with the donor offsets; `kqueue()` and `kqueue1(O_CLOEXEC\|O_NONBLOCK)` succeed; an undefined `kqueue1` flag is `EINVAL` |
| `read-ready` | a pipe's read end is not ready before a byte, then reports `EVFILT_READ` with no `EV_ERROR` |
| `socket-ready` | a `socketpair` end reports `EVFILT_READ` after the peer writes |
| `read-write-same-fd` | `EVFILT_READ` and `EVFILT_WRITE` on one descriptor register together and both are reported |
| `modify-udata` | re-adding an existing filter with `EV_ADD` is a modify and replaces `udata` |
| `disable-enable` | `EV_DISABLE` suppresses the record; `EV_ENABLE` restores it |
| `oneshot` | `EV_ONESHOT` deletes the registration after its first record |
| `timer` | `EVFILT_TIMER` fires, reports `EVFILT_TIMER` and a positive `data` |
| `user` | `EVFILT_USER` with `NOTE_TRIGGER` reports the registration's `fflags` |
| `unsupported-filter` | `EVFILT_SIGNAL` returns a per-change `EV_ERROR` with `EOPNOTSUPP`, never an empty success |
| `unsupported-fflags` | `NOTE_LOWAT` on `EVFILT_READ` returns `EOPNOTSUPP` because the queue has no watermark |
| `receipt` | `EV_RECEIPT` returns one `EV_ERROR` record with `data == 0` |
| `close-reuse` | closing the registered descriptor and reusing its number does not deliver the old object's event |
| `eintr` | a signal interrupts a blocking wait with `EINTR` (S711) |

## Controlled mutations

| Mutation | Row it must turn red |
|---|---|
| `retain-after-close` | `close-reuse` (a registration that survives close and reuse) |
| `unsupported-filter-success` | `unsupported-filter` (an unsupported filter reported as registered) |
| `lose-oneshot` | `oneshot` (the registration not deleted after its record) |

## Donor map

FreeBSD donor `133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause), Illumos
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` (CDDL, behaviour only) and Linux
`adc218676eef25575469234709c2d87185ca223a` (GPL, outcome only).

| Topic | Illumos finding | BSD file + licence | Linux finding | Decision |
|---|---|---|---|---|
| declared filter set | `port_associate`/`port_get` event source model | FreeBSD `sys/sys/event.h` `EVFILT_*`, `sys/kern/kern_event.c` `sysfilt_ops`/`kqueue_fo_find` (BSD-2) | `fs/eventpoll.c` outcome only | test only: exactly the pinned numbers; outside the provider returns `EOPNOTSUPP` |
| registration/modify/delete | port association replace semantics | FreeBSD `sys/kern/kern_event.c` `kqueue_register` (existing knote found, `EV_DELETE` drops, `EV_ADD` modifies, missing without `EV_ADD` is `ENOENT`) (BSD-2) | outcome only | test only: modify keeps `EV_CLEAR`, replaces `udata` unless `EV_KEEPUDATA` |
| enable/disable and one-shot | port source disable | FreeBSD `kqueue_register` `KN_DISABLED`, `kqueue_scan` `EV_ONESHOT` drop (BSD-2) | outcome only | test only: disabled source silent, one-shot deleted |
| timer | timer source deadline | FreeBSD `filt_timervalidate`/`filt_timerattach`/`filt_timertouch` (data 0 is one unit, `NOTE_ABSTIME` supported) (BSD-2) | outcome only | test only: relative timer; `NOTE_ABSTIME` is a documented `EOPNOTSUPP` |
| user event | doorbell level | FreeBSD `filt_userattach`/`filt_usertouch`/`filt_user` (BSD-2) | outcome only | test only: `NOTE_TRIGGER` and the stored `fflags`; `NOTE_FF*` control is `EOPNOTSUPP` |
| descriptor lifetime | port association follows the port object | FreeBSD `kern_event.c` `knote_fdclose`, `filt_piperead`, `filt_soread` (BSD-2) | outcome only | test only: close and reuse a number; the old object's event is not delivered |
| EINTR | signal interrupts the wait | FreeBSD `kqueue_scan` `PCATCH`/`EINTR` (BSD-2) | outcome only | test only: `kevent` returns `EINTR` |

No donor expression is copied; the probe asserts observable records and errno values.

## Run

```sh
cargo xtask libc-pack --arch x86_64
python3 tests/conformance/harness/runner.py --topic tests/conformance/kqueue \
    --service kqueue-conformance
python3 tests/conformance/harness/runner.py --topic tests/conformance/kqueue \
    --service kqueue-conformance --phase mutation --mutation retain-after-close
```
