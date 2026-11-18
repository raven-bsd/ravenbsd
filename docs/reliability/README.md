<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD reliability evidence — S85g (rework round 2)

- Date: 2026-11-18
- Product base: `6ff7c106` (origin/master `94648948` merged into `jerry/S85g`)
- Status: **PARTIAL / umbrella BLOCKED.** The Raven-side evidence is refreshed; the S85 SMP
  matrix is still red and DR-49 is not met. No release reliability claim.
- Rule: measured numbers only. A missing or flaky matrix is **BLOCKED**, never a pass.

## Evidence matrix (current master)

| Ticket | Harness / evidence | State at this tree | Key measured result / blocker | Owner |
|---|---|---|---|---|
| S85a locks/time/IPIs | `tests/smp/lock-time/**` | **BLOCKED** (harness merged) | TCG `-smp 4`: 1 pass, 2/3 50-iteration samples fail with `smp: tlb shootdown timeout` (`kernel/src/smp/shootdown.rs:122`); pass p90 1.086 ms / max 11.688 ms | lance; kernel fix S83/stuart |
| S85b memory/paging/TLB | `tests/smp/memory/**` | **BLOCKED** (harness merged) | TCG `-smp 4`: 3/3 fail, 88/85/90 forbidden TLB-timeout lines; `shootdown.rs:81` `IrqSpinLock` suspected | mark; kernel fix S83/stuart |
| S85c IPC/rings/tracing | `tests/smp/ipc/**` | **BLOCKED** (TCG green; KVM legs pre-S83) | TCG `-smp 4`: 5/5 pass, 12,800 calls + 25,600 ring descriptors, p90 31.6 ms-2.887 s, worst 88.1 ms-4.061 s; KVM 2/4 stopped before APs at its base | tim |
| S85d POSIX/pthread/libc | `tests/libc/target/**`, qtest cases | **BLOCKED** (harness merged) | `libc-threads` 20/20 TCG timeouts (60.8-66.5 s), malloc/stdio concurrency 3/3 fail (`fputs` EOF), `posix-alarm` 6/6 pass; KVM legs pre-S83 | otto; libc-thread/stdio lanes |
| S85e VFS/filesystem | `tests/posix/fs/posix-fs/**` | **BLOCKED** (harness merged) | TCG `-smp 4`: real concurrency failures (EBADF mmap, read-after-write mismatch, rename/mount ENOMEM, stuck workers); one-worker control 50/50 pass | mel; vfsd/ramfsd/kernel IPC |
| S85f aarch64/TCG | `scripts/smp-aarch64.sh`, `build/qtest/smp-aarch64/` | **BLOCKED** | aarch64 TCG: `smp-tlb-aarch64` 20/23 (3 TLB flakes at `kernel/src/sched/smp_shim.rs:225`); x86 `libc-threads` hang 2/2; aarch64 KVM unavailable in QEMU 6.2 | carl; dave/C57, S83 |
| S83 KVM AP bring-up | `sh scripts/qemu-kvm.sh` | **DONE** | KVM `-smp 1/2/4` PASS with login (this rework) | stuart |
| S84 boot-time | `sh scripts/boot-time.sh --smp 4` | **DONE, flaky under load** | 3-run sample A: kernel→login median 1,532 ms (FAIL); sample B: median 534 ms / p90 1,081 ms (PASS); committed S84 at `4ceb76b3`: 409 ms | dave |
| S89 direct boot | `sh scripts/boot-direct.sh` | **BLOCKED** | exit 2: `Error loading uncompressed kernel without PVH ELF Note`; owner Stuart | kevin/stuart |
| S95 network | `scripts/qemu-net.sh`; release-login | **DONE** | S95 report; `qemu-kvm` login and release path pass | bob |
| S96 brush/no-rsh | S96 report | **BLOCKED** | brush login/completion/no-rsh change not landed | phil |

The umbrella is not green: five S85 campaigns are BLOCKED on measured TCG failures/flakes, S96 is
blocked, and the direct-boot path is absent. Every failed log is kept by the owning lane; none is
retried into a pass.

## Fresh S85g measurements (OBSERVED on `6ff7c106`)

| Command | Result |
|---|---|
| `sh scripts/boot-time.sh --smp 4` (sample A) | **FAIL**: firmware 8,489/19,114/10,474 ms; kernel→login 1,085/2,021/1,532 ms (median 1,532 ms); total 9,575/21,135/12,007 ms |
| `sh scripts/boot-time.sh --smp 4` (sample B) | **PASS**: firmware 5,086-10,500 ms; kernel→login 1,081/534/426 ms (median 534 ms, p90 1,081 ms); total 5,512-11,582 ms |
| `sh scripts/qemu-kvm.sh --no-build --image build/boot-time/release.img` | **PASS** `-smp 1/2/4` with login |
| `sh scripts/boot-time.sh --tcg --runs 1` | kernel→login 3,689 ms; total 12,362 ms (report only) |
| `sh scripts/boot-direct.sh --arch x86_64` | **exit 2**: no PVH ELF note; exact blocker/owner printed |

Image: `build/boot-time/release.img`, 320 MiB, SHA-256
`7987e189325dc55208fb8da4966faa62ffa2ea035bcd8bc6dc16eb4f412578f9`, kernel banner `6ff7c106`.
Host: QEMU 6.2.0, OVMF 2022.02, KVM `kvm_intel`, i7-12700H, Linux 5.19.0-50. The two boot-time
samples are both real; the difference is host load (other minion QEMU runs), so the gate is
**flaky** and cannot be reported as a clean pass.

## DR-52 donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| QEMU/KVM boot-time measurement | SMF boot phases/milestones (`boot`, `svc.startd`); design-state machine, no code | FreeBSD `usr.sbin/bhyve` (BSD-2) is a VMM, not a timing harness; no permissive boot-time comparator to take as-is | systemd boot-phase timing and `perf` outcomes; read-only | First-party shell harness (`scripts/boot-time.sh`) with QEMU/serial markers; no donor code copied |
| SMP AP bring-up (needed for the benchmark) | `mp_startup`, `xcalls`, TLB shootdown protocols; design-only | FreeBSD `mp_machdep.c` / NetBSD `cpu.c` (per-file BSD; reference only) inform the bring-up; S83 uses a clean-room Raven layout | `smpboot.c`, `tlb.c` behaviour/outcome; read-only | S83 clean-room fix (kernel AP root/trampoline); no donor code; see the S83 report |
| SMP stress campaigns | `xcalls`, turnstiles, TLB ack protocols; design-only | NetBSD ATF (BSD-2) is the existing test-harness donor; the S85 harnesses are first-party | scheduler/IPC/TLB stress outcomes; read-only | First-party fixtures under `tests/smp/**`; exact commands and failures in each S85 report |
| Boot-phase table (firmware/kernel/login) | boot milestones; no code | FreeBSD loader/rc phases (BSD-2) as a naming reference | systemd-analyze phase split; read-only | First-principles markers: firmware, kernel→login and total reported separately; no borrowed numbers |
| BSD/Linux comparison images | not applicable | FreeBSD release image would be the BSD comparator | Linux distro image would be the Linux comparator | **Owner/reviewer input missing:** no frozen image set or checksums in the repo; no comparison run |

No donor source was copied, translated or closely paraphrased in this ticket. The measurement
harnesses are first-party; the S83 fix is clean-room.

## Exact blockers

| Blocker | Owner / file | Needed for |
|---|---|---|
| TCG TLB shootdown timeout/flakes (S85a/b/f) | stuart / S83, `kernel/src/smp/shootdown.rs`, `kernel/src/sched/smp_shim.rs:225` | the SMP stress matrix |
| pthread/libc-threads hangs; malloc/stdio concurrency | otto + libc-thread/stdio lanes | S85d |
| VFS/IPC concurrency failures | mel + stuart/tim (vfsd/ramfsd/router) | S85e |
| S85c KVM legs measured before S83; rerun on this tree | tim | S85c KVM rows |
| Boot-time flake (1,532 ms sample vs 534 ms sample) | dave / S84, host load + service path | the <1 s OS gate |
| Direct boot: no PVH ELF note | stuart / S89, kernel entry | the <1 s full QEMU gate |
| BSD/Linux image set and checksums | owner/reviewer | the apples-to-apples comparison |

## Reproduction

```sh
sh scripts/boot-time.sh --smp 4                 # KVM 4-CPU gate (two samples above)
sh scripts/qemu-kvm.sh --no-build --image build/boot-time/release.img
sh scripts/boot-direct.sh --arch x86_64        # exit 2: no PVH note
sh scripts/boot-time.sh --tcg --runs 1         # TCG report only
```

Verdict: **PARTIAL** for S85g's documented evidence (the Raven-side is measured and the blockers
are exact); the S85 umbrella remains **BLOCKED**, DR-49 is not met, and no release/tag claim is
made.
