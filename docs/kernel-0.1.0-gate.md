<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel 0.1.0 gate (S80)

Recorded "hammer everything" sweep at commit `822c027f` (`dave/S80`, from `origin/master`).
No product code changed; this is evidence only.

Result: **BLOCKED**. Everything the host and QEMU can exercise is green except two items:

1. `proc-death-cleanup` fails on x86-64 **and** aarch64 with a kernel data abort / double fault in
   the process-teardown path. This is the same shared regression A8b recorded; owner: shared
   kernel — memory/frames (mark) with the scheduler lane (stuart).
2. The Q4 real-server trace is incomplete: the **`vfsd` span is missing** (Q4e, P5c lane / phil).
   `trace-demo` proves `client → fsd-ufs → blockd` on a direct `ufs:` open; a normal mounted read
   through `vfsd` cannot yet produce the `client → vfsd → fsd-ufs → blockd` tree.

The 24 h fault-injection soak is also not completed: the ticket's `--soak 24h` is invalid for the
script (it takes **seconds**), and a 24 h foreground run cannot fit a session. A bounded
`--soak 180` smoke passed 3/3; the exact long command is recorded below.

## 1. Full gate

| Command | Exit | Result |
|---|---|---|
| `make check` | 0 | **PASSED** (guard, fmt, clippy host, 11897 host tests, doc tests, profiles, provenance, kernel x86_64/aarch64 clippy, user clippy, loader clippy, docs) |
| `sh scripts/kernel-gate.sh` | 1 | 26/27 pass; `proc-death-cleanup` FAIL (see §2) |
| `sh scripts/abi-diff.sh --self-test` | 0 | `abi-diff: ok (table current)` |
| `cargo xtask qtest --case proc-death-cleanup --arch x86_64` | 1 | **FAIL** (see §2) |
| `cargo xtask qtest --case smp-user --arch x86_64` | 0 | pass |
| `cargo xtask qtest --case service-restart --arch x86_64` | 0 | pass |
| `cargo xtask qtest --case fault-restart --arch x86_64` | 0 | pass |
| `cargo xtask qtest --case trace-demo --arch x86_64` | 0 | pass |
| `cargo xtask qtest --case cap-revoke --arch x86_64` | 0 | pass |
| `cargo xtask qtest --case time-entropy --arch x86_64` | 0 | pass |
| `cargo xtask qtest --case release-login --arch x86_64` | 0 | pass |
| `sh scripts/fault-injection.sh --profile release --arch x86_64 --soak 24h` | 0 | **NOT A PASS** — `[: Illegal number: 24h`, 0 runs, 0 pass, 0 fail (see §3) |
| `sh scripts/fault-injection.sh --profile release --arch x86_64 --soak 180` | 0 | 3 runs, 3 pass, 0 fail, `first_failure=none` |
| `cargo xtask qtest --case proc-death-cleanup --arch aarch64` | 1 | **FAIL** (same regression as §2) |
| `cargo xtask qtest --case smp-user-aarch64 --arch aarch64` | 0 | pass |
| `cargo xtask qtest --case smp-user-off-aarch64 --arch aarch64` | 0 | pass |
| `cargo xtask qtest --case smp-tlb-aarch64 --arch aarch64` | 0 | pass |
| `cargo xtask qtest --case smp-soak-aarch64 --arch aarch64` | 0 | pass |
| `cargo xtask qtest --case smp-soak-off-aarch64 --arch aarch64` | 0 | pass |
| `cargo xtask qtest --case fault-restart --arch aarch64` | 0 | pass |
| `cargo xtask qtest --case fault-reconnect --arch aarch64` | 0 | pass |
| `cargo xtask qtest --case trace-demo --arch aarch64` | 0 | pass |

### `scripts/kernel-gate.sh` rows (26 pass / 1 fail)

x86-64: `boot-uname`, `kernel-early-boot`, `exc-int3`, `exc-div0`, `exc-pagefault`,
`exc-doublefault`, `kernel-paging`, `kernel-timer`, `time-entropy`, `time-entropy-cyclic`,
`smp-user`, `smp-user-off`, `irq-dist`, `irq-dist-off`, `locking`, `lockstat`, `ipc-donation`,
`kmem-frames`, `kmem-frames-nopcpu`, `trace-context`, `release-login` — pass;
`proc-death-cleanup` — FAIL.

aarch64: `boot-uname-aarch64`, `kernel-early-boot-aarch64`, `kernel-exc-aarch64`,
`kernel-paging-aarch64`, `kernel-timer-aarch64` — pass.

The script's own table (`docs/kernel-gate.md`) was regenerated as a side effect and reverted, because
this ticket owns only `docs/kernel-0.1.0-gate.md`; the per-case evidence is in
`target/kernel-gate/results.txt` and the case logs.

## 2. Red: `proc-death-cleanup` (owner: shared kernel)

x86-64 (exit 35 from qtest; kernel double fault):

```
EXCEPTION: page fault (vector 14, error 0x0)
EXCEPTION: double fault (vector 8, error 0x0)
unmatched: ^s69: cycle 25$
```

aarch64 (exit 1; identical to the A8b record):

```
exception: sync-cur-spx EC=0x25 (data abort same EL) ESR=0x0000000096000047 ELR=0xffffffff80234a2c FAR=0xffffc00000020000 SPSR=0x00000000000003c5
exception: halted
```

`FAR=0xffffc00000020000` is page-aligned and the ELR is inside `__exception_vectors`: a kernel
stack exhausted to its guard page during `s69` teardown. Owner: memory/frames (mark) with the
scheduler lane (stuart); the aarch64 arch lane is not implicated because x86-64 fails the same way.

## 3. Fault-injection soak

The ticket's command passes `--soak 24h`, but `scripts/fault-injection.sh` parses `--soak` as a
**number of seconds** and compares it with `[ ... -le 0 ]` / `[ ... -lt ... ]`. With `24h` both
tests report `Illegal number`, the loop body never runs, and the script still exits 0:

```
scripts/fault-injection.sh: 66: [: Illegal number: 24h
scripts/fault-injection.sh: 75: [: Illegal number: 24h
fault-injection: soak done pass=0 fail=0 first_failure=none
```

That is a false green, so it is recorded as **NOT A PASS**. The correct 24 h invocation is
`sh scripts/fault-injection.sh --profile release --arch x86_64 --soak 86400`; a 24 h foreground
run does not fit this session and was not attempted. As bounded evidence the same command with
`--soak 180` ran three complete `fault-restart-release` cycles:

```
fault-injection: run pass=1 fail=0 recovery_ms=82499
fault-injection: run pass=2 fail=0 recovery_ms=54399
fault-injection: run pass=3 fail=0 recovery_ms=47376
fault-injection: soak done pass=3 fail=0 first_failure=none
```

## 4. Q4 real-server trace (BLOCKED)

`trace-demo` passes on both architectures, but the recorded tree is
`client → fsd-ufs → blockd` for a direct `ufs:` open. A normal mounted read goes through `vfsd`,
which has no probe yet, so the real-server demo cannot show
`client → vfsd → fsd-ufs → blockd`. The missing span is the **`vfsd` server span** (the `vfs`
operation), tracked as Q4e in `docs/trace-demo.md` §4 and owned by the P5c lane (phil). Q4c
(fsd-ufs/blockd probes, bob `71643199`) and Q4d (`raven_rt::trace`, bob `adfd16db`) are merged; Q4e
is not filed or merged, so the gate row stays BLOCKED rather than green.

## 5. Pi 4

UNVERIFIED. No USB-UART adapter is attached (`/dev/ttyUSB*` absent) and no board log exists, so the
serial/GENET gates cannot be claimed. The host/QEMU side is recorded separately in
`docs/pi4-qemu-gate.md` (A8c): image check and the four host-backed pi4 cases pass, board rows
UNAVAILABLE.

## 6. Environment notes (clone state, not product defects)

Two staged sysroots in this clone were stale and made the first runs red for the wrong reason;
regenerating them (generated files only) fixed the runs:

* `build/x86_64/libc.a` (03:50) lacked K2 exports such as `basename`, `iconv`, `syslog`,
  `strfmon`, so `raven-libc::archive_exports a1_k1_to_k6_pass` failed. `cargo xtask libc-pack
  --arch x86_64` regenerated the sysroot and `make check` then passed.
* `build/aarch64/libc.a` was already regenerated during A8c.

`xtask initfs::ensure_libc` only checks that the sysroot files exist, not that they are current,
so a long-lived clone keeps stale archives and the host `archive_exports` test reads
`build/x86_64/libc.a`. Worth a follow-up ticket; no source file was edited for it here.
