<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel QEMU gate

Generated for current master `6d1b1bde`
(`6d1b1bdedf6803dd328bf142d6d9818276ca37a6`) from the S113 kernel-gate evidence:
`sh scripts/kernel-gate.sh` ran on S113 at 2026-11-18T18:52:21Z from commit `ed613f72`
(`ed613f72dc340a1f499dbf1fd3e0177f5b9bd8c1`). This is the S113 run replayed into the document;
no fresh QEMU run was made for KG1.

Result: **PASS** — 27 cases, 27 passed, 0 failed (0 timed out): x86_64 22/22, aarch64 5/5.

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
