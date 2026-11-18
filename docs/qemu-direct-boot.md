<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU direct boot: measurement and kernel entry contract

Date: 2026-11-18. Ticket: S89. Owner decision: kernel entry (Stuart) now, full gate after
S84/S95/S96.

## Why

DR-49 requires the **full QEMU boot including firmware** to reach brush login in under a second.
The UEFI/OVMF path is the only working route today and its firmware stage alone is far over the
gate. This note records the UEFI baseline, the exact QEMU direct-boot blocker, and the entry
contract needed to finish the direct route without bypassing init/services.

## Measured baseline (OBSERVED)

Host: `qemu-system-x86_64` **6.2.0**, `/dev/kvm` present and QEMU advertises `kvm`, but the
timed table uses **TCG** (the kernel-gate default). Image: `build/x86_64/ravenbsd-x86_64.img`
(the current 335 MB release image). 3 runs, wall clock from QEMU start to the serial line:

| Path | Kernel banner | Login | Notes |
|---|---|---|---|
| UEFI/OVMF (TCG) | median **38.2 s**, p90 **54.3 s** (runs 31.4/38.2/54.3) | median **166.0 s**, p90 **182.8 s** (159.2/166.0/182.8) | OVMF + large image; the lead's ~3.4 s firmware figure is not reproduced on this shared host |
| Direct `-kernel` (microvm) | **never** | **never** | QEMU exits 75-141 ms with the error below |

The TCG table is dominated by OVMF/disk setup and the 335 MB image on this host, not by the
kernel; the kernel-to-login part is not the bottleneck. A KVM timed run was attempted but QEMU
exited after ~18 serial lines without a kernel banner in this clone; KVM is therefore **not**
claimed as a measured path here.

## Direct-boot blocker (exact)

```
$ qemu-system-x86_64 -machine microvm -m 512M -smp 4 \
    -kernel build/x86_64/kernel.elf -append 'console=ttyS0' \
    -serial stdio -display none -monitor none -no-reboot
qemu-system-x86_64: Error loading uncompressed kernel without PVH ELF Note
```
The kernel ELF has no multiboot header and no **PVH note**
(`XEN_ELFNOTE_PHYS32_ENTRY`, ELF note type 18). `scripts/boot-direct.sh` runs this probe, reports
the elapsed time and exits 2 with this contract; it never fakes a banner or a login.

## Required kernel entry contract (for Stuart, S83 lane)

`kernel/src/arch/x86_64/start.rs` `_start` requires, on entry:

* long mode, paging on, interrupts off;
* a loader-provided stack;
* `rdi` = **HHDM virtual address** of a valid `raven_bootinfo::BootInfo`.

QEMU PVH enters in **32-bit protected mode** with `%ebx` = physical address of the
`hvm_start_info` (memory map, modules, cmdline). A direct route therefore needs one of:

1. **Kernel PVH shim (preferred):** add the PVH note pointing at a 32-bit entry; the shim parses
   `hvm_start_info`, builds the page tables/HHDM, copies the kernel/initfs modules (QEMU
   `-initrd`/module list) and cmdline, assembles `BootInfo`, then enters `_start` in long mode
   with `rdi` as above.
2. **First-stage loader in `boot/**`:** a multiboot2/PVH ELF whose entry loads the kernel and
   initfs as modules, builds the same `BootInfo`/page tables, and jumps to the kernel `_start`.

Required `BootInfo` fields (`crates/raven-bootinfo/src/lib.rs`): `magic`, `version`, `size`,
`arch`, `flags`, `hhdm_offset`, `kernel_phys_base`, `kernel_virt_base`, `kernel_size`,
`memory_map_base/len/entry_size`, `initfs`, `cmdline`, `dtb`, `acpi_rsdp`,
`uefi_system_table` (0 for direct), `boot_cpu_id`, `entry_counter`, `counter_freq`,
`framebuffer`. The HHDM base and paging layout are in `boot/loader/src/layout.rs` and
`boot/loader/src/paging*`; those modules are reusable, but `image`/`memmap`/`handoff` currently
compile only for the UEFI target.

## Options and consequence

* **PVH/32-bit shim in the kernel:** smallest runtime path; QEMU can then start the kernel in
  milliseconds. Needs kernel code, an ELF note and a module/initfs handoff; owner Stuart.
* **First-stage loader in `boot/**`:** keeps the kernel entry unchanged; adds a loader binary and
  module loading to the direct QEMU command; larger surface but inside this ticket's `boot/**`.
* **Minimal firmware instead of OVMF:** not recommended; no supported path here reaches <1 s
  including firmware, and a fake firmware would bypass the real image/services.

Either option must boot the **same release image/userland** (brush login, completion, network,
no rsh) once S84/S95/S96 land. `init`, services, and the initfs must not be bypassed.

## Commands

```
# UEFI baseline / current working path
cargo xtask image --arch x86_64 --profile release
cargo xtask run --arch x86_64 --profile release

# Direct probe (currently exits 2 with the PVH blocker)
sh scripts/boot-direct.sh --arch x86_64

# Full gate once S84/S95/S96 and scripts/boot-time.sh land
sh scripts/boot-time.sh

# UEFI/TCG regression
sh scripts/kernel-gate.sh
```

## Dependencies / status

* S84/S95/S96 are the boot-time/service/network/brush tickets; their completion is required for
  the full <1 s login gate. `scripts/boot-time.sh` is not present in this tree yet.
* This note does not change the kernel, SMP/AP path, services or gate rows. The direct path is
  **BLOCKED** until the kernel PVH entry (or a first-stage loader) exists.
