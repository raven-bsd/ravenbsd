<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel 0.1.0 gate (S80)

Recorded gate at commit `6d806e8f` (`dave/S80` fast-forwarded to `origin/master`). No product code
changed; this is evidence only.

Kernel/QEMU gate: **green 27/27**. The two remaining S80 items are both recorded here:

1. the **aarch64 release image does not build** on this tree (the P1b libc defect is still present),
2. the **24 h soak was not executed**; this round ran the longest bounded soak it allows (15 min,
   15 runs) and does not label it as 24 h.

## 1. Gate table

| Command | Exit | Result | Owner of failure |
|---|---|---|---|
| `cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release` | 2 | **FAIL** at the aarch64 libc sysroot step (see §2) | kevin (P1b) |
| `sh scripts/kernel-gate.sh` | 0 | **PASS (27 cases)** | — |
| `cargo xtask qtest --case release-login --arch x86_64` | 0 | pass | — |
| `sh scripts/fault-injection.sh --profile release --arch x86_64 --soak 15m` | 0 | **15 runs, 15 pass, 0 fail** (not 24 h, see §3) | — |
| `make check-fast` | 0 | **`check-fast: PASSED`** | — |
| 24 h soak (`--soak 24h` = `86400`) | — | **NOT EXECUTED** (does not fit a session) | — |
| Pi 4 board serial/GENET | — | **UNVERIFIED** (no adapter, no board log) | board owner |

`kernel-gate.sh` rows (all pass): x86-64 `boot-uname`, `kernel-early-boot`, `exc-int3`, `exc-div0`,
`exc-pagefault`, `exc-doublefault`, `kernel-paging`, `kernel-timer`, `time-entropy`,
`time-entropy-cyclic`, `smp-user`, `smp-user-off`, `irq-dist`, `irq-dist-off`, `locking`,
`lockstat`, `ipc-donation`, `kmem-frames`, `kmem-frames-nopcpu`, `proc-death-cleanup`,
`trace-context`, `release-login`; aarch64 `boot-uname-aarch64`, `kernel-early-boot-aarch64`,
`kernel-exc-aarch64`, `kernel-paging-aarch64`, `kernel-timer-aarch64`.

`proc-death-cleanup` stays green (L6b `f023fe93`); the round-2 evidence was 10/10 on x86-64 and 3/3
on aarch64, and this round's gate passes it again. `trace-demo` keeps the four-span
`client → vfsd → fsd-ufs → blockd` tree from Q4e (`0a3fab65`).

## 2. aarch64 release image: FAILS (P1b still open)

```
$ cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release
==> libc aarch64: packing missing prerequisites (build/aarch64/libc.a, build/aarch64/crt1.o, ...)
error[E0599]: no associated function or constant named `from_dtoa_bytes` found for struct `LongDouble128`
  --> libc/ravenlibc/crates/raven-libc/src/stdlib/strtold.rs:24:39
error[E0599]: no associated function or constant named `from_dtoa_bytes` found for struct `LongDouble128`
  --> libc/ravenlibc/crates/raven-libc/src/wchar/wcstold.rs:30:39
error: could not compile `raven-libc` (lib) due to 2 previous errors
exit status 2
```

`from_dtoa_bytes`/`to_dtoa_bytes` exist only in the x86-64 `long_double` module; on the binary128
targets `LongDouble` is `binary128::LongDouble128`, which has no such bridge, and
`strtold`/`wcstold` call it unconditionally. The P1b/P1g/P1h merges on `6d806e8f` did not add the
bridge. Because the aarch64 C sysroot cannot be packed, `brush` cannot link and the Pi 4 release
image cannot be produced. **The blocker stays open.**

The x86-64 sysroot packs normally (`cargo xtask libc-pack --arch x86_64` → `sysroot: ok`), so
`release-login`, the x86-64 kernel rows and `make check` are unaffected. The aarch64 kernel and the
Rust `no_std` aarch64 qtests are unaffected too, which is why the kernel gate is green.

## 3. Soak: 15 minutes, 15 runs (not 24 h)

```
fault-injection: run pass=1 fail=0 recovery_ms=51852
fault-injection: run pass=2 fail=0 recovery_ms=53854
... (13 more passing runs, pass=3 .. pass=14)
fault-injection: run pass=15 fail=0 recovery_ms=62071
fault-injection: soak done pass=15 fail=0 first_failure=none
```

`--soak 15m` is 900 s; fifteen complete `fault-restart-release` iterations, all passing, exit 0.
This is **not** the 24 h soak: the 24 h form is `--soak 24h` (or `86400`) and was **not executed**
in this round, because a 24 h foreground run does not fit a session. No shorter run is reported as
24 h.

## 4. Environment

* The staged sysroots were stale after the merge; the x86-64 one was regenerated
  (`cargo xtask libc-pack --arch x86_64`) so `raven-libc::archive_exports` sees the current K2
  exports. The aarch64 one cannot be regenerated (§2).
* Regenerable qtest images live on `/tmp` via the git-ignored `build/qtest` path because `/home` is
  a shared, mostly full filesystem.
* One stale `qemu-system-x86_64` belongs to another session (2 h 46 m old, `base-tools`); it was
  left alone. The stale QEMU from the interrupted S80 round 1 was already stopped.

## 5. Pi 4

UNVERIFIED. No `/dev/ttyUSB*` adapter and no board capture exist, so serial/GENET are not claimed.
Host/QEMU evidence is in `docs/pi4-qemu-gate.md` (A8c).
