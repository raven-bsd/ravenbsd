<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc ucontext pointer ABI

C64k. Status: fix implemented and host-tested; the target C loop passes its checks, while the
qtest case itself still needs the C73c harness pieces described below.

## The fault (OBSERVED)

S23 localized the `libc-signals` page fault at `0x0` to the public context entry points. The shell
exported them as raw tail jumps:

```asm
getcontext:  jmp __rl_getcontext
setcontext:  jmp __rl_setcontext
swapcontext: jmp __rl_swapcontext
```

The hidden assembly operates on an `mcontext_t *`, which is embedded in the C `ucontext_t` at
`ucontext + UC_MCONTEXT_WORD * 8` (`raven-libc-signal/src/frame.rs`). On x86-64 that is 40 bytes
(word 5); on aarch64 and RISC-V it is 176 bytes (word 22). The C ABI passes the `ucontext_t *`
unchanged, so `getcontext` wrote a register image into the `uc_link`/`uc_stack` area and
`swapcontext` restored a `ucontext_t` field as RIP, jumping to `0`.

## The fix (OBSERVED)

`raven-libc/src/ucontext/context_abi.rs` is the new pointer and `errno` layer. It calls the public
signal functions, which add the `mcontext_t` offset and maintain the signal mask:

* `getcontext(ucontext_t *)` -> `raven_libc_signal::context::getcontext`.
* `setcontext(const ucontext_t *)` -> `raven_libc_signal::context::setcontext` (never returns).
* `swapcontext(ucontext_t *, const ucontext_t *)` -> `raven_libc_signal::context::swapcontext`.

A NULL context returns `-1` with `errno = EFAULT` (`Errno(14)`); no pointer is dereferenced.
`ucontext/mod.rs` keeps only the `#[unsafe(no_mangle)] extern "C"` exports and delegates to this
file. `makecontext` is unchanged; it already calls `makecontext_core`.

The target build links the same code: `cargo xtask libc-pack --arch x86_64` succeeds and the
signals test links against the resulting `libc.a`.

## Tests (OBSERVED)

### Host regression

`tests/libc/ucontext` is a standalone crate. The shell's `ucontext` module is target-only
(`#[cfg(target_os = "none")]` in `raven-libc/src/lib.rs`), so this crate includes the real
`context_abi.rs` source and supplies only the `ffi::ret::int` shim.

```
$ cargo test --manifest-path tests/libc/ucontext/Cargo.toml
running 3 tests
test tests::the_mcontext_starts_at_ucontext_plus_forty_bytes ... ok
test tests::null_contexts_report_efault ... ok
test tests::a_thousand_makecontext_swapcontext_round_trips ... ok
test result: ok. 3 passed; 0 failed; 0 ignored
```

The loop test is the same shape as the C73 `check_ucontext`: a worker context is prepared with a
64 KiB stack and `uc_link`, then the main context and worker `swapcontext` 1000 times. The test
asserts exactly 1000 worker entries. `the_mcontext_starts_at_ucontext_plus_forty_bytes` pins the
offset that caused the fault.

`cargo test -p raven-libc` also passes (171 unit tests plus the 13 `archive_exports` tests); it
does not compile the target-only module, which is why the standalone host crate above exists.

### Target case

The exact acceptance command on this base reports the C73c skip gate:

```
$ cargo xtask qtest --case libc-signals --arch x86_64
libc-signals: skipped
  needs PG-1 PG-2 PG-13 PG-22 PG-23 PG-24 PG-29 PG-50 PG-53
```

C73c owns unlocking that case. To get execution evidence without editing `tests/qtest/cases/**` or
`tests/libc/target/**`, a scratch copy of the case with the `skip` removed was run together with
two scratch harness pieces that C73c adds but this base does not yet have: a page-aligned C
linker script (the strict loader rejects the old lld tail) and a `DEBUG_WRITE` fallback in the
test header (the base header cannot reach the console). Both scratch files live outside the tree.

The serial output from that run (`build/c64k-qtest-signals-run3.log` name in the work log) is:

```
libc-signals: start
libc-signals: sigset ok
libc-signals: sigaction ok
libc-signals: raise ok
libc-signals: mask ok
libc-signals: setjmp ok
libc-signals: ucontext ok
libc-signals: sigaltstack ok
libc-signals: SKIP fault ok needs PG-7 PG-8 PG-12
libc-signals: SKIP pthread_kill ok needs PG-7 PG-8
libc-signals: SKIP eintr ok needs PG-7 PG-8
libc-signals: SKIP sa_restart ok needs PG-7 PG-8
libc-signals: SKIP sigtimedwait ok needs PG-7 PG-8
libc-signals: SKIP alarm ok needs PG-7 PG-19
libc-signals: SKIP fp ok needs PG-7 PG-12
libc-signals: done
process: child exited (0)
```

`ucontext ok` is only printed when `check_ucontext` returns true, which requires
`context_swaps == 1000`; the fault is gone and the loop completed. The qtest verdict on that
scratch run is still `fail` for one non-C64k reason: `^init exited \(0\)$` is unmatched because
`initfsd`/`vfsd` hit their readiness timeout with the base 55 MiB `libc-test` profile and `init`
exits 1. C73c's minimal profile is the fix; it is outside this ticket's Owns. There is no
`page fault`, `EXCEPTION`, or `libc-signals: FAIL` line in the run.

## Limits

* The host crate proves the pointer/`errno` layer and the 1000-iteration round trip on the host's
  x86-64; it does not boot RavenBSD.
* The target scratch run proves the C path; the committed tree does not contain the C73c linker
  script, `DEBUG_WRITE` header fallback or minimal profile, so `cargo xtask qtest
  --case libc-signals --arch x86_64` still reports `skipped` until C73c lands.
* No other libc crate, `Symbol.map`, or `tests/libc/target/**` file was changed. `libc-pack`
  rewrites `libc/ravenlibc/abi/Symbols.list`; `cargo xtask libc-abi gen` restores the canonical
  1133-record file after packing.

## Evidence tags

* OBSERVED: the S23 fault, the `UC_MCONTEXT_WORD` values, the host test results, the scratch target
  serial markers and the `init exited (1)` cause.
* PROPOSED: C73c owns the case unlock and the profile/header/linker fixes.
* UNVERIFIED: the exact acceptance qtest verdict after C73c lands without the PG gates.
