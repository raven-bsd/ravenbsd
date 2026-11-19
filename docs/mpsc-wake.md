<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# std mpsc send blocks: pthread object-size mismatch

Status: **BLOCKED — the std `libc-ravenbsd` pthread object sizes do not match the Raven libc.**
The fix is in the Rust target libc shim and needs a toolchain/std rebuild; it is outside the K155
Owns paths. The bounded fixture and qtest are added as the regression.

## Reproduction

`tests/qtest/cases/mpsc-wake.toml` runs the std fixture `/bin/mpsc-wake` as a normal shell child in
the `shell-test` image (valid std fds; the earlier custom `/init` aborts in
`std::sys::pal::unix::init`'s `sanitize_standard_fds`, which is a separate harness limitation).
The fixture runs a `Mutex`/`Condvar` ping-pong control first, then 20 unbounded `mpsc::channel`
rounds and 20 `mpsc::sync_channel(1)` rounds; each receiver waits before the send.

Current result:

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
implementations (`raven-libc-thread`) use the musl-compatible sizes:

| object | Raven libc | std `libc-ravenbsd` (patched 0.2.185) |
|---|---|---|
| `pthread_attr_t` | 56 (`Attr`, const assert) | **32** (`_PTHREAD_ATTR_SIZE`) |
| `pthread_mutex_t` | 40 (`Mutex`, const assert) | **12** (`_PTHREAD_MUTEX_SIZE`) |
| `pthread_cond_t` | 48 (`Cond`, const assert) | **8** (`_PTHREAD_COND_SIZE`) |

The Redox-derived constants are in `toolchain/vendor/libc/src/unix/ravenbsd/mod.rs` (and the
installed source patch `0002-libc-ravenbsd.patch`; the uutils copy
`userland/uutils/libc-ravenbsd` has the same values). The std allocates an 8-byte `Condvar` and a
12-byte `Mutex`; `pthread_cond_wait`/`pthread_mutex_lock` treat them as 48/40-byte `Cond`/`Mutex`
and write past them, corrupting the channel/tuple. The thread spawn's 32-byte attr is overrun by
the 56-byte `Attr`, which is the first corruption in the fixture. The page fault at 0x0 is the
corrupted pointer, not a kernel futex lost wake.

## Fix and handoff

Set the target libc's pthread sizes to the Raven libc C ABI: `_PTHREAD_ATTR_SIZE = 56`,
`_PTHREAD_MUTEX_SIZE = 40`, `_PTHREAD_COND_SIZE = 48` (and align the remaining `_PTHREAD_*`
sizes with `raven-libc-abi`'s generated structs). Update
`toolchain/vendor/libc/src/unix/ravenbsd/mod.rs`, the `0002-libc-ravenbsd.patch` source, and the
uutils copy; then the lead rebuilds/reinstalls the `x86_64-unknown-ravenbsd` std (`make toolchain`
is lead-owned). Handoff: **Kevin** (target libc/toolchain); no kernel futex change is needed.
No K141/K139/K148 path is touched.

After the std rebuild, the same fixture/qtest is the acceptance: the condvar control and 20/20
unbounded + 20/20 bounded mpsc rounds must pass; the K148 synchronous `du` printer can then be
revisited by its follow-up.

## Regression

`tests/qtest/cases/mpsc-wake.toml` + `tests/qtest/cases/mpsc-wake/` (std fixture and the host
initfs repack tool). The case is the default-suite regression once the toolchain fix lands; it
currently fails with the page fault above and should not enter the default suite before then.

## Donor reconciliation (DR-52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| pthread object ABI | Illumos pthread types are opaque musl-like objects; behaviour only, no code read | BSD pthread types are opaque and libc-owned; behaviour only, no code copied | Linux/glibc pthread types are opaque libc-owned; sizes are the libc ABI | Use the Raven libc's existing 56/40/48 ABI; fix the target libc shim, no donor code copied or translated. |

## Provenance

First-party BSD-3-Clause investigation. No donor code was read or copied. The fixture links the
existing target std and the first-party libc; the host repack tool uses `raven-initfs` only.
