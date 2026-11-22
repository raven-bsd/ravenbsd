<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel QEMU gate

Status: **PASS** on the S140 run at current master `f803502a`
(`f803502af5b03721fec27c87771e2c93792e84f6`). Command:
`/usr/bin/time -p timeout 620 sh scripts/kernel-gate.sh`; elapsed **541.92 s**, cap **620 s**.

Cap arithmetic: timed-out 21-case x86 sum **267.343 s** + measured `release-login` **285.78 s**
+ aarch64 JSON sum **31.256 s** + **30 s** margin = **614.379 s** -> 620 s. The release-login
prebuild was measured separately: release image warm **130.32 s**; `check-loader.sh` warm
**133.78 s**; release-login alone **285.78 s**. No blanket cap was used.

Host telemetry (`target/kernel-gate/host-state.txt`): load1 **4.54**, nproc 20, max_load 5,
qemu none, jobs none, UTC `2026-11-19T03:17:55Z`.

Result: **PASS** — 27 cases, 27 passed, 0 failed (0 timed out): x86_64 **22/22**, aarch64
**5/5**. Logs: `target/kernel-gate/<case>.<arch>.log`; table from
`target/kernel-gate/results.txt`. This is a fresh current-commit run, not a replay.

Scope: the release-relevant kernel cases below, x86_64 plus the aarch64 cases that exist
in `tests/qtest/cases/`. Raspberry Pi 4 hardware is **out of scope**; this gate makes no Pi 4
claim. Each row is one `cargo xtask qtest --case <case> --arch <arch>` run in the foreground.
A timeout is a failure. Full serial/build logs are in `target/kernel-gate/<case>.<arch>.log`
(not committed).

| Case | Arch | Owner | Subsystem | Result | Failure |
|---|---|---|---|---|---|
| boot-uname | x86_64 | stuart | boot/uname | **pass** | ok |
| kernel-early-boot | x86_64 | stuart | boot | **pass** | ok |
| exc-int3 | x86_64 | stuart | exceptions | **pass** | ok |
| exc-div0 | x86_64 | stuart | exceptions | **pass** | ok |
| exc-pagefault | x86_64 | stuart | exceptions | **pass** | ok |
| exc-doublefault | x86_64 | stuart | exceptions | **pass** | ok |
| kernel-paging | x86_64 | stuart | paging | **pass** | ok |
| kernel-timer | x86_64 | stuart | timer | **pass** | ok |
| time-entropy | x86_64 | lance | time-entropy | **pass** | ok |
| time-entropy-cyclic | x86_64 | lance | time-entropy | **pass** | ok |
| smp-user | x86_64 | stuart | smp | **pass** | ok |
| smp-user-off | x86_64 | stuart | smp | **pass** | ok |
| irq-dist | x86_64 | stuart | irq | **pass** | ok |
| irq-dist-off | x86_64 | stuart | irq | **pass** | ok |
| locking | x86_64 | lance | locking | **pass** | ok |
| lockstat | x86_64 | lance | locking | **pass** | ok |
| ipc-donation | x86_64 | lance | ipc | **pass** | ok |
| kmem-frames | x86_64 | mark | allocator | **pass** | ok |
| kmem-frames-nopcpu | x86_64 | mark | allocator | **pass** | ok |
| proc-death-cleanup | x86_64 | stuart | process | **pass** | ok |
| trace-context | x86_64 | tim | trace | **pass** | ok |
| release-login | x86_64 | kevin | release | **pass** | ok |
| boot-uname-aarch64 | aarch64 | dave | boot/uname | **pass** | ok |
| kernel-early-boot-aarch64 | aarch64 | dave | boot | **pass** | ok |
| kernel-exc-aarch64 | aarch64 | dave | exceptions | **pass** | ok |
| kernel-paging-aarch64 | aarch64 | dave | paging | **pass** | ok |
| kernel-timer-aarch64 | aarch64 | dave | timer | **pass** | ok |

## Failures by owner

None.
