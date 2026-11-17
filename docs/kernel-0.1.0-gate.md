<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel 0.1.0 gate (S80)

Recorded "hammer everything" sweep at commit `9e52217f` (`dave/S80`, the merge of
`origin/master` `0a3fab65` into the gate branch). No product code changed; this is evidence only.

Result: the kernel/QEMU gate is **green 27/27**, every listed acceptance command passes, and the
Q4 four-span trace is included. One product finding below is outside the gate's acceptance list but
blocks the aarch64 release image, so the overall aarch64 release claim is **not** green yet.

## 1. Gate table

| Command | Exit | Result | Owner of failure |
|---|---|---|---|
| `make check` | 0 | **PASSED** (guard, fmt, host clippy, 11 900 host tests, doc tests, profiles, provenance, kernel x86_64/aarch64 clippy, user clippy, loader clippy, docs) | — |
| `sh scripts/kernel-gate.sh` | 0 | **PASS (27 cases)** | — |
| `sh scripts/abi-diff.sh --self-test` | 0 | `abi-diff: ok (table current)` | — |
| `qtest proc-death-cleanup --arch x86_64` | 0 | pass, **10/10** consecutive runs | — |
| `qtest proc-death-cleanup --arch aarch64` | 0 | pass, 3/3 runs | — |
| `qtest smp-user --arch x86_64` | 0 | pass | — |
| `qtest service-restart --arch x86_64` | 0 | pass | — |
| `qtest fault-restart --arch x86_64` | 0 | pass (one early-exit flake in six runs; see §5) | otto (S71 fixture), non-blocking |
| `qtest trace-demo --arch x86_64` | 0 | pass, four-span tree (§3) | — |
| `qtest cap-revoke --arch x86_64` | 0 | pass | — |
| `qtest time-entropy --arch x86_64` | 0 | pass | — |
| `qtest release-login --arch x86_64` | 0 | pass | — |
| `sh scripts/fault-injection.sh --profile release --arch x86_64 --soak 2m` | 0 | **3 runs, 3 pass, 0 fail** (§2) | — |
| `sh tests/scripts/check-fault-injection-duration.sh` | 0 | `fault-injection-duration-test: PASS` | — |
| `qtest smp-user-aarch64 --arch aarch64` | 0 | pass | — |
| `qtest smp-user-off-aarch64 --arch aarch64` | 0 | pass | — |
| `qtest smp-tlb-aarch64 --arch aarch64` | 0 | pass | — |
| `qtest smp-soak-aarch64 --arch aarch64` | 0 | pass | — |
| `qtest smp-soak-off-aarch64 --arch aarch64` | 0 | pass | — |
| `qtest fault-restart --arch aarch64` | 0 | pass | — |
| `qtest fault-reconnect --arch aarch64` | 0 | pass | — |
| `qtest trace-demo --arch aarch64` | 0 | pass, four-span tree (§3) | — |
| `make check-fast` | 0 | **`check-fast: PASSED`** | — |
| Pi 4 board serial/GENET | — | **UNVERIFIED** (no adapter, no board log) | board owner |

`kernel-gate.sh` rows (all pass): x86-64 `boot-uname`, `kernel-early-boot`, `exc-int3`, `exc-div0`,
`exc-pagefault`, `exc-doublefault`, `kernel-paging`, `kernel-timer`, `time-entropy`,
`time-entropy-cyclic`, `smp-user`, `smp-user-off`, `irq-dist`, `irq-dist-off`, `locking`,
`lockstat`, `ipc-donation`, `kmem-frames`, `kmem-frames-nopcpu`, `proc-death-cleanup`,
`trace-context`, `release-login`; aarch64 `boot-uname-aarch64`, `kernel-early-boot-aarch64`,
`kernel-exc-aarch64`, `kernel-paging-aarch64`, `kernel-timer-aarch64`.

`proc-death-cleanup` is fixed by L6b (`f023fe93`): the A8b/S80-round-0 page fault and double fault
no longer reproduce, 10/10 on x86-64 and 3/3 on aarch64.

`check-fast` summary:

```
check-fast summary:
  ok       guard (exit 0, 7.51s)
  ok       fmt (exit 0, 7.71s)
  ok       clippy (host) (exit 0, 0.76s)
  ok       tests (host, parallel) (exit 0, 73.49s)
  ok       tests (doc) (exit 0, 4.57s)
  ok       profiles (exit 0, 0.08s)
  ok       provenance (exit 0, 39.35s)
  skipped  clippy kernel x86_64 (--fast)
  skipped  clippy kernel aarch64 (--fast)
  skipped  clippy user programs x86_64 (--fast)
  skipped  clippy loader x86_64 (--fast)
  ok       docs (exit 0, 0.10s)
check-fast: PASSED
```

The full `make check` above also ran the target-clippy stages that `--fast` skips (all `ok`).

## 2. Fault-injection soak

`scripts/fault-injection.sh` now parses `--soak` as a duration: `<n>`, `<n>s`, `<n>m`, `<n>h` or
`<a>h<b>m` (bare `<n>` is seconds), so the ticket's `24h` is **86400 s** rather than an
`Illegal number` that silently ran nothing. Anything else exits 2 with the value in the message, a
budget that would run zero iterations is rejected, and the driver exits 2 if the loop completed no
runs. The parser table, the usage-error paths and the zero-run guard are covered by both
`--self-test` and `tests/scripts/check-fault-injection-duration.sh`.

Bounded run on this tree (the acceptance command):

```
fault-injection: run pass=1 fail=0 recovery_ms=54606
fault-injection: run pass=2 fail=0 recovery_ms=57797
fault-injection: run pass=3 fail=0 recovery_ms=55260
fault-injection: soak done pass=3 fail=0 first_failure=none
```

Three real `fault-restart-release` iterations, three passes, no failure. The 24 h form is
`--soak 24h` (or `86400`); a 24 h foreground run does not fit a session and was not attempted, and
that is the only soak row that is not complete.

## 3. Q4 four-span trace (Q4e merged)

`trace-demo` passes on **both** architectures with the real server stack
(`initfsd → blockd → fsd-ufs → vfsd`) and the mounted `file:/mnt/ten.txt` read:

```
vfsd: mounted / on initfs:
vfsd: mounted /mnt on ufs:
trace: spans=13 gaps=0 traced=11 untraced=2
trace: id=0xdeadbeefcafef00d span=0x1 ... pid=0 kind=server op=1   # vfsd open (producer 1)
trace: id=0xdeadbeefcafef00d span=0x2 ... pid=0 kind=server op=1   # fsd-ufs open (producer 2)
trace: id=0xdeadbeefcafef00d span=0x3 ... pid=0 kind=server op=1   # fsd-ufs open
trace: id=0xdeadbeefcafef00d span=0x2 ... pid=0 kind=server op=2   # vfsd read
trace: id=0xdeadbeefcafef00d span=0x4 ... pid=0 kind=server op=2   # fsd-ufs read
trace: id=0xdeadbeefcafef00d span=0x1 ... pid=0 kind=io     op=7   # blockd block I/O (producer 3)
trace: id=0xdeadbeefcafef00d span=0x5 ... pid=0 kind=server op=1   # warm direct ufs: read
trace: id=0xdeadbeefcafef00d span=0x6 ... pid=0 kind=server op=2
demo: ok
```

The `pid=2` records are the fixture server's traced/untraced proof spans; the `pid=0` records are
the real servers (userspace has no `getpid`, the kernel owns pids). The qtest asserts every span
line, so `client → vfsd → fsd-ufs → blockd` is now part of the gate rather than a blocked row. See
`docs/trace-demo.md` for the span mapping.

## 4. Finding outside the acceptance list: aarch64 libc build is broken

While regenerating the staged aarch64 sysroot (the clone's archives predate the merge),
`cargo xtask libc-pack --arch aarch64` fails to compile `raven-libc`:

```
error[E0599]: no associated function or constant named `from_dtoa_bytes` found for struct `LongDouble128`
  --> libc/ravenlibc/crates/raven-libc/src/stdlib/strtold.rs:24:39
error[E0599]: no associated function or constant named `from_dtoa_bytes` found for struct `LongDouble128`
  --> libc/ravenlibc/crates/raven-libc/src/wchar/wcstold.rs:30:39
```

`from_dtoa_bytes`/`to_dtoa_bytes` live only in the `#[cfg(target_arch = "x86_64")]` `long_double`
module; on the binary128 targets `LongDouble` is `binary128::LongDouble128`, which has no such
bridge. `strtold`/`wcstold` call it unconditionally. Owning commit: `234d6cfa` (P1b, lane kevin).

Effect: the aarch64 C sysroot (and therefore the aarch64 release image, which stages `brush`)
cannot be built on this tree. The kernel and the Rust `no_std` aarch64 qtests above are unaffected,
which is why the kernel gate is green; the aarch64 **release image** is not. Owner: kevin (P1b).
This is why the overall result is not DONE.

## 5. Flakes and environment

* `fault-restart` (x86-64) failed once out of six runs: the controller exited 0 right after
  `read-before=ok`, so the restart markers were missing. Five subsequent runs passed. Non-blocking;
  the fixture is S71 (otto).
* An interrupted earlier session left a stale `qemu-system-x86_64` holding
  `build/qtest/fault-restart-release.vars.fd` for ~80 minutes. That made `fault-restart-release`
  exit in 5 s with zero serial output ("no serial output before the failure"); after stopping the
  stale process the case and the 2 m soak passed. Environment artifact, not a product defect.
* The clone's staged sysroots were stale after several merges; `xtask initfs::ensure_libc` only
  checks that `libc.a`/`crt*.o` exist, so `raven-libc::archive_exports` reads an old
  `build/x86_64/libc.a`. Regenerating it with `cargo xtask libc-pack --arch x86_64` restored the
  green `make check`. Worth a follow-up ticket: the host test should build or refuse a stale
  archive rather than trust it.
* `/home` is a shared, nearly full filesystem (other sessions); the regenerable qtest images live
  on `/tmp` through the git-ignored `build/qtest` path.

## 6. Pi 4

UNVERIFIED. No `/dev/ttyUSB*` adapter and no board capture exist, so the serial/GENET gates are not
claimed. The host/QEMU side is recorded in `docs/pi4-qemu-gate.md` (A8c): the image check and the
four host-backed pi4 QEMU cases pass; board rows UNAVAILABLE.
