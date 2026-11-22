<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Poll readiness contract

Status: **in progress (K426)**. A `poll`/`ppoll` call answers from the source's **current** state.
A wakeup, doorbell or previously delivered mask is only a hint to re-check; it is never the
verdict. A source whose live query cannot answer must block until a real state change and wake,
not report a fabricated readable/writable bit.

## Contract

1. **Live query at poll time.** Before sleeping, and again after every wake, the poll loop asks
   the source for its current level: `Call::Poll` on the socket's **data handle** (not the
   attached event handle), the kernel level predicate for kernel-backed sources.
2. **Doorbell = hint.** `EV_SIGNALED`, a delivered `EV_READABLE`/`EV_WRITABLE` mask, a stored
   rearm mask and an `EVQ_WAIT` event wake the loop; they do not set `revents` by themselves.
3. **No fabricated scheme bits.** If a scheme server cannot answer `Call::Poll`, the kernel does
   not invent `READABLE|WRITABLE` for the scheme file. The caller stays blocked until a real
   state-change wake; the live query then supplies the answer.
4. **Cached masks.** Any cached mask (exec rearm, inherited fd) is an optimisation that can only
   wake/recheck. It must not outlive a state change; the current implementation keeps it as a
   one-shot rearm hint (`take_poll_rearm`) and the poll loop still runs the live query.
5. **Level correctness.** Drain-to-empty, a new write, a window update and a post-fork/exec
   descriptor must all be observed from the current state, not from the previous edge.

## Implementation map

| Layer | File | Behaviour |
|---|---|---|
| libc poll loop | `libc/ravenlibc/crates/raven-libc-platform/src/poll/poll.rs` | live scan before sleep, after each wait and on spurious wake; scheme failures never fall through to the event mask |
| OFD handle | `.../fd/ofd.rs` | `handle()` is the data handle; `poll_handle()` is the event hint when attached |
| net readiness | `libc/ravenlibc/crates/raven-libc-net/src/ready.rs` | current socket interest/revents mapping; K409 handle refresh after fork |
| kernel level | `kernel/src/syscall/event.rs` | kernel-backed level predicate; live scheme file returns 0 (no invented level) |
| netd | `userland/servers/netd/src/main.rs` | live `Call::Poll`; readable+writable doorbell, every bell per file |
| K409 | `kevin/K409` (cherry-picked) | live query uses the data handle; writability wakes; post-fork handle refresh |

## Donor map

FreeBSD is the primary behavioural reference (kqueue level filters evaluate current state);
NetBSD/Illumos/Linux are cross-checks. The Raven event seam is first-party and the FreeBSD code
was not copied: the files below were read for the predicate/wake contract only. If a later
transplant is desired, the exact functions are named for a per-file provenance entry.

| Donor | Revision | Exact files/functions read | Use |
|---|---|---|---|
| FreeBSD | `133344405f532bd54ff3629a77d4c4e062f01fdb` | `sys/kern/uipc_sockbuf.c:469 sowakeup`, `:541 sorwakeup_locked`, `:559 sowwakeup_locked`; `sys/kern/uipc_socket.c:4512 sopoll_generic`, `:4629 filt_soread`, `:4687 filt_sowrite`; `sys/kern/sys_generic.c:1955 selrecord`, `:2003 selwakeup`, `:131 doselwakeup`; `sys/kern/kern_event.c:2557 knote`, `:2970 knote_enqueue`; `sys/sys/socketvar.h` socket buffer state | primary contract: level filter evaluates current state; wake only notifies |
| NetBSD | `36eddebcc36cb0775aa2962005b9c05419a6ff01` | `sys/kern/uipc_socket.c` `sowakeup`/`sopoll`; `sys/kern/sys_generic.c` `selrecord`/`selwakeup`; `sys/kern/sys_poll.c`; `sys/sys/socketvar.h` | cross-check where simpler |
| Illumos | `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` | `usr/src/uts/common/fs/sockfs/sockcommon_sops.c` `so_poll`; `pollhead`/`pollwakeup` contract | CDDL, design-only; no copy |
| Linux | current master (read-only) | `fs/select.c` poll/level, `fs/eventpoll.c` level vs edge | behavioural oracle in tests; no code |

## Test matrix

| Case | Test | Status |
|---|---|---|
| signaled doorbell, live query unavailable | `a_signaled_doorbell_without_a_live_answer_is_not_readable` | PASS (new; red before fix) |
| stored rearm mask is not a verdict | `a_stored_readiness_mask_is_not_a_poll_verdict` | PASS |
| live query uses the data handle after exec | K409 `socket-write-exec` fixture | pending qtest |
| writability wake | K409 `socket-write-exec` / tap | pending qtest |
| drain then refill | `net-two-reads` | pending qtest |
| pipe level | `pipe-poll` | pending qtest |
| socket echo | `socket-inet` | pending qtest |
| accepted TX | `virtio-tx` | pending qtest |
| SSH 10/10 | K195 tap gate | outside this minion run |

## Open gaps

1. There is no synchronous kernel scheme `poll` op, so the kernel cannot forward
   `SchemeFile` readiness; it returns 0 and relies on a wake. This is the contract's "if
   unavailable, block and wake" branch, but a scheme source with no attached event can still
   sleep indefinitely; a future kernel scheme poll op is required.
2. The qtest matrix (`socket-inet`, `net-two-reads`, `pipe-poll`, `virtio-tx`,
   `socket-write-exec`) and the SSH 10/10 gate were not run in this minion session; the host
   unit tests and check-fast are the evidence here.
3. The cached rearm mask is a hint in the current poll path; a full generation-stamp counter is
   not added because no verdict path consumes the mask. If K245/other consumers start reading it,
   it needs the generation stamp.
4. FreeBSD `kqueue`/`selinfo` was read behaviourally; no literal FreeBSD file was transplanted,
   so no new `PROVENANCE.toml` entry is claimed.
