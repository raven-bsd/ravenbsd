<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel QEMU gate

Status: **BLOCKED — TIMEOUT** on the S140 attempt at current master `f803502a`
(`f803502af5b03721fec27c87771e2c93792e84f6`). Command: `timeout 500 sh scripts/kernel-gate.sh` (cap 500 s = S113 prior
441 s + 59 s margin). Host telemetry: load1 1.02, nproc 20, max_load 5, qemu none, jobs none.
Result: 21 of 22 x86_64 cases passed; `release-login` did not finish before the cap and the
aarch64 five cases were not reached. Preserved logs: `target/kernel-gate/release-login.x86_64.log`
and `target/kernel-gate/host-state.txt`. No 27/27 or current-commit PASS is claimed.

The table below is the historical S113 run at `ed613f72`; it is not the current-commit result.

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
