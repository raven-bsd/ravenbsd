<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# std mpsc send blocks: pthread object-size mismatch

Status: **source fix complete (K158), runtime gate pending the lead's toolchain rebuild.**
The three target-libc copies now match the Raven ABI; `tests/toolchain/pthread-abi/check.sh`
passes on the host. The installed `x86_64-unknown-ravenbsd` std still has the old sizes until the
lead rebuilds the patched toolchain.

## Reproduction

`tests/qtest/cases/mpsc-wake.toml` runs the std fixture `/bin/mpsc-wake` as a normal shell child in
the `shell-test` image (valid std fds; an earlier custom `/init` aborts in
`std::sys::pal::unix::init`'s `sanitize_standard_fds`, which is a separate harness limitation).
The fixture runs a `Mutex`/`Condvar` ping-pong control first, then 20 unbounded `mpsc::channel`
rounds and 20 `mpsc::sync_channel(1)` rounds; each receiver waits before the send.

Pre-fix result (K155, installed std):

```text
raven# /bin/mpsc-wake
mpsc-wake: start
process: child killed by page fault at 0x0
process: child exited (-11)
```

The condvar/thread-spawn control faults before the mpsc rounds. The K148 `du` hang (the main
blocks in `mpsc::Sender::send` while the printer waits in `rx.recv`) is the same corruption with a
smaller object set.

## Root cause: object sizes

The Raven libc's C ABI (`libc/ravenlibc/crates/raven-libc-abi/src/generated/structs.rs`) and the
implementations (`raven-libc-thread`) use the musl-compatible objects. The std allocated the
Redox-derived tiny objects; `pthread_*` writes past them and corrupts the channel/tuple. The
32-byte `pthread_attr_t` is overrun by the 56-byte `Attr` at thread spawn (the first corruption),
then the mutex/condvar overruns follow. The page fault at 0x0 is the corrupted pointer, not a
kernel futex lost wake.

| object | size / align | `_PTHREAD_*` constant |
|---|---|---|
| `pthread_attr_t` | 56 / 8 | `_PTHREAD_ATTR_SIZE` |
| `pthread_mutex_t` | 40 / 8 | `_PTHREAD_MUTEX_SIZE` |
| `pthread_cond_t` | 48 / 8 | `_PTHREAD_COND_SIZE` |
| `pthread_rwlock_t` | 56 / 8 | `_PTHREAD_RWLOCK_SIZE` |
| `pthread_barrier_t` | 32 / 8 | `_PTHREAD_BARRIER_SIZE` |
| `pthread_mutexattr_t` | 4 / 4 | `_PTHREAD_MUTEXATTR_SIZE` |
| `pthread_condattr_t` | 4 / 4 | `_PTHREAD_CONDATTR_SIZE` |
| `pthread_barrierattr_t` | 4 / 4 | `_PTHREAD_BARRIERATTR_SIZE` |
| `pthread_rwlockattr_t` | 8 / 4 | `_PTHREAD_RWLOCKATTR_SIZE` |
| `pthread_once_t` | 4 / 4 | `_PTHREAD_ONCE_SIZE` |
| `pthread_spinlock_t` | 4 / 4 | `_PTHREAD_SPINLOCK_SIZE` |

Before the fix the three copies had attr 32/8, mutex 12/4, cond 8/4, rwlock 4/4, barrier 24/4,
mutexattr 20/4, condattr 8/4, barrierattr 4/4, rwlockattr 1/1, once 4/4, spin 4/4.

## Fix (K158)

The same table is applied in all three copies; the frozen Redox donor modules are not edited and
there is no fourth copy:

- `toolchain/vendor/libc/src/unix/ravenbsd/mod.rs`
- `toolchain/patches/0002-libc-ravenbsd.patch` (Raven section only; the Redox section at the next
  `+++ b/library/libc-ravenbsd/src/unix/redox/mod.rs` header is untouched)
- `userland/uutils/libc-ravenbsd/src/unix/ravenbsd/mod.rs`

`pthread_once_t`/`pthread_spinlock_t` are verified against the `raven-libc-thread` asserts
(`Once` 4, `SpinLock` 4); their sizes were already correct.

`tests/toolchain/pthread-abi/check.sh` reads the authoritative sizes/alignments from
`raven-libc-abi` and fails on any divergence between the three copies. It runs on the host
without the patched toolchain:

```text
$ timeout 60 sh tests/toolchain/pthread-abi/check.sh
type                     abi       vendor    patch     uutils
pthread_attr_t           56/8      56/8      56/8      56/8
pthread_mutex_t          40/8      40/8      40/8      40/8
pthread_cond_t           48/8      48/8      48/8      48/8
pthread_rwlock_t         56/8      56/8      56/8      56/8
pthread_barrier_t        32/8      32/8      32/8      32/8
pthread_mutexattr_t      4/4       4/4       4/4       4/4
pthread_condattr_t       4/4       4/4       4/4       4/4
pthread_barrierattr_t    4/4       4/4       4/4       4/4
pthread_rwlockattr_t     8/4       8/4       8/4       8/4
pthread_once_t           4/4       4/4       4/4       4/4
pthread_spinlock_t       4/4       4/4       4/4       4/4
target-libc copies with the pthread size table: 3
pthread-abi: ok (three copies match raven-libc-abi)
```

## Toolchain handoff

The installed std is unchanged until the lead runs the patched toolchain rebuild (lead-owned):

```text
make toolchain
```

No kernel futex change is needed; no K141/K139/K148 path is edited.

## Runtime acceptance (after the lead rebuild)

```text
timeout 120 cargo xtask qtest --case mpsc-wake --arch x86_64
timeout 120 cargo xtask qtest --case base-tools-du --arch x86_64
timeout 120 cargo xtask qtest --case smp-libc-concurrency --arch x86_64
timeout 120 cargo xtask qtest --case release-login --arch x86_64
timeout 240 make check-fast
```

The condvar control and 20/20 unbounded + 20/20 bounded mpsc rounds must pass. If they do,
`base-tools-du` decides whether K148's synchronous printer can be reverted by its follow-up.

## Regression

`tests/qtest/cases/mpsc-wake.toml` + `tests/qtest/cases/mpsc-wake/` (std fixture and the host
initfs repack tool). The case stays out of the default suite until the toolchain rebuild passes
the after-fix gate above.

## Donor reconciliation (DR-52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| pthread object ABI | Illumos pthread types are opaque musl-like objects; behaviour only, no code read | BSD pthread types are opaque and libc-owned; behaviour only, no code copied | Linux/glibc pthread types are opaque libc-owned; sizes are the libc ABI | Use the Raven libc's existing 56/40/48 ABI; fix the target libc shim, no donor code copied or translated. |

## Provenance

First-party BSD-3-Clause investigation. No donor code was read or copied. The fixture links the
existing target std and the first-party libc; the host repack tool uses `raven-initfs` only.
