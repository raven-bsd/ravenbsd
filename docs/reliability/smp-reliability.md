<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP reliability evidence (S85g, rework round 2)

- Date: 2026-11-18
- Product base: `6ff7c106` (origin/master `94648948` merged into `jerry/S85g`)
- Host: i7-12700H, Linux 5.19.0-50-generic, QEMU 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31),
  OVMF 2022.02-3ubuntu0.22.04.7, `/dev/kvm` + `kvm_intel`
- Image: `build/boot-time/release.img`, 320 MiB, SHA-256
  `7987e189325dc55208fb8da4966faa62ffa2ea035bcd8bc6dc16eb4f412578f9`, banner `6ff7c106`
- Status: **BLOCKED** for the S85 matrix. The harnesses exist; the campaigns are red.

## Stress matrix (the S85a-f reports)

Each row is the owning lane's committed harness/report at this tree; failures are kept, not
retried into a pass.

| Sub-ticket | Harness | Mode / runs | Result | Measured failure / key number | Owner / file |
|---|---|---|---|---|---|
| S85a | `tests/smp/lock-time/**` | TCG `-smp 4`, three 50-iteration samples | **BLOCKED** (1 pass, 2 fail) | pass: p90 1,085,771 ns, max 11,688,178 ns, 4,480 hops; fails at iter 39 and iter 4: `smp: tlb shootdown timeout on cpu 0/2`, no latency samples | lance; stuart / `kernel/src/smp/shootdown.rs:122` |
| S85b | `tests/smp/memory/**` | TCG `-smp 4`, 3 runs | **BLOCKED** (3/3 fail) | 88/85/90 forbidden TLB-timeout lines; durations 15,150/24,790/13,947 ms; `shootdown.rs:81` IrqSpinLock masked interrupts while a second caller spins | mark; stuart / `kernel/src/smp/shootdown.rs:81` |
| S85c | `tests/smp/ipc/**` | TCG `-smp 4`, 5 consecutive passes | **BLOCKED** (KVM legs pre-S83; TCG green) | 12,800 calls, 25,600 ring descriptors, 12,000 trace spans; p90 31.6 ms-2.887 s, worst 88.1 ms-4.061 s; KVM 2/4 stopped at `timer: 1000 Hz` at its base | tim |
| S85d | `tests/libc/target/**`, qtest cases | TCG `-smp 4`, 20-iteration time-box + 300 s case | **BLOCKED** | `libc-threads` 20/20 timeouts (60.8-66.5 s; hang at start/mutex/cond/once/tls/barrier); malloc/stdio concurrency 3/3 fail (`fputs` EOF, errno 0); `posix-alarm` 6/6 pass; KVM legs pre-S83 | otto; libc-thread/stdio lanes |
| S85e | `tests/posix/fs/posix-fs/**` | TCG `-smp 4`, 50 iterations | **BLOCKED** | EBADF mmap (workers 0/1 phase 9), read-after-write mismatch, rename/mount ENOMEM, remount stale state, workers stuck; one-worker control 50/50 pass; mount p90 89 µs / max 1.36 s | mel; vfsd/ramfsd + stuart/tim kernel IPC |
| S85f | `scripts/smp-aarch64.sh`, `build/qtest/smp-aarch64/` | aarch64 TCG `-smp 4`, 23 runs | **BLOCKED** | `smp-tlb-aarch64` 20/23 (3 TLB flake, 10 %; `kernel/src/sched/smp_shim.rs:225`), `smp-user-aarch64` 23/23; x86 `libc-threads` hang 2/2; aarch64 KVM unavailable in QEMU 6.2 | carl; dave/C57, S83 |

The KVM legs in S85a/b/d/e were measured on the pre-S83 base; S83 now passes KVM `-smp 1/2/4`
(see below), so those legs must be rerun on this tree before they can be green. The TCG failures
above are independent of the KVM AP fix.

## KVM AP bring-up (S83) — now PASS

`sh scripts/qemu-kvm.sh --no-build --image build/boot-time/release.img --timeout 120`:

```
qemu-kvm: PASS -smp 1
qemu-kvm: PASS -smp 2
qemu-kvm: PASS -smp 4
qemu-kvm: PASSED -smp 1/2/4 with login
```

KVM `-smp 2/4` reaches `smp: N cpus online` and login on the current release image; the earlier
`timer: 1000 Hz` stop is fixed by S83. This removes the old KVM blocker for the benchmark, but
not the stress campaigns' TCG failures.

## Boot-time samples (Raven-side)

`sh scripts/boot-time.sh --smp 4` (KVM 4 CPU, 3 runs per sample):

| Sample | firmware (ms) | kernel→login (ms) | total (ms) | Gate |
|---|---|---:|---:|---|
| A run 1/2/3 | 8,489 / 19,114 / 10,474 | 1,085 / 2,021 / 1,532 | 9,575 / 21,135 / 12,007 | **FAIL** (median 1,532 ms) |
| B run 1/2/3 | 10,500 / 6,648 / 5,086 | 1,081 / 534 / 426 | 11,582 / 7,182 / 5,512 | **PASS** (median 534 ms, p90 1,081 ms) |

The committed S84 table at `4ceb76b3` records kernel→login median **409 ms** / p90 425 ms. The
two samples here differ by ~3× on the kernel segment while the firmware also varies by ~4×; the
host was running other minion QEMUs. The gate is therefore **flaky** on this host and is not
reported as a clean pass. `--tcg --runs 1` reports kernel→login 3,689 ms / total 12,362 ms
(report only). The full QEMU start→login is far above 1 s because OVMF is 5-19 s; the S89 direct
path is the only way to the full gate and is blocked (below).

## Direct boot (S89)

`sh scripts/boot-direct.sh --arch x86_64` exits 2:

```
boot-direct: QEMU exit=1 in 11 ms
boot-direct: qemu-system-x86_64: Error loading uncompressed kernel without PVH ELF Note
boot-direct: BLOCKED: the kernel ELF has no PVH note (XEN_ELFNOTE_PHYS32_ENTRY)
  required: a 32-bit PVH entry ... or a first-stage multiboot/PVH loader.
  owner: Stuart (kernel entry/S83 lane)
```

No direct-boot number exists; the full QEMU <1 s gate fails on firmware.

## Reproduction

```sh
sh scripts/boot-time.sh --smp 4
sh scripts/qemu-kvm.sh --no-build --image build/boot-time/release.img --timeout 120
sh scripts/boot-direct.sh --arch x86_64
sh tests/smp/lock-time/run.sh tcg            # S85a; keep the serial log on failure
sh tests/smp/memory/run.sh tcg --iterations 1 # S85b
sh tests/smp/ipc/run.sh tcg 4                 # S85c
sh tests/libc/target/build-smp.sh             # S85d fixture
cargo xtask qtest --case posix-fs-smp --arch x86_64  # S85e
sh scripts/smp-aarch64.sh --tcg               # S85f
```

No test was weakened and no failure was retried into a pass. The umbrella remains BLOCKED until
the TLB/pthread/VFS owners land fixes and every campaign passes 50/50 or the documented time-box.
