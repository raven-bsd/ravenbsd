<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel QEMU gate

Generated for current master `1111111111111111111111111111111111111111`
(`1111111111111111111111111111111111111111`) from the KG1 evidence:
`sh scripts/kernel-gate.sh` ran at 2026-11-18T00:00:00Z from commit `1111111111111111111111111111111111111111`.

Result: **PASS** — 26 cases, 26 passed, 0 failed (0 timed out).

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
