<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU direct boot: first-stage loader, measurements and remaining blockers

Date: 2026-11-18. Ticket: S89 (round 2). Owner of the loader: kevin. Kernel entry unchanged.

## What exists now

`boot/direct` is a small PVH ELF that QEMU can start with `-kernel`:

* `entry.S` is the 32-bit PVH entry. It builds a 4-level identity map for the first 4 GiB,
  enables PAE/LME/NX/paging, loads a flat 64-bit GDT and calls the Rust entry with the
  `hvm_start_info` physical address.
* `src/main.rs` parses the PVH start information, the `-initrd` module and the Raven kernel ELF,
  places the kernel image, converts the PVH memory map into `raven_bootinfo::MemoryRegion`s,
  builds the HHDM plus kernel page tables, fills `BootInfo` and jumps to the existing kernel
  `_start` with `rdi` = HHDM pointer to `BootInfo`.
* The loader does not edit the kernel, the SMP/AP path, the UEFI loader, services or gate rows.

`scripts/boot-direct.sh` builds the loader, creates the module and runs the real probe. The
module is:

```
"RVDIRECT" (8 bytes) | padded kernel length (u64 LE) | kernel ELF | zero padding | initfs
```

The kernel length is padded so the initfs starts on a 4 KiB boundary inside the QEMU initrd.
The kernel maps the initfs into user space, so a page-aligned base is required; without the
padding `raven_rt::map_initfs()` returns `EINVAL` and `svcd` exits 1. This is why the payload is
built by the script rather than by concatenating the two files blindly.

## Commands

```sh
# Kernel-banner probe (defaults: microvm, KVM when /dev/kvm is usable, -smp 1, release initfs)
sh scripts/boot-direct.sh --arch x86_64

# Same probe with TCG
sh scripts/boot-direct.sh --arch x86_64 --tcg

# Full release-image path: q35 + virtio-net, waits for `login:`
sh scripts/boot-direct.sh --mode full

# Rebuild the kernel and the release initfs first
sh scripts/boot-direct.sh --rebuild

# UEFI/TCG regression (the working image path)
sh scripts/kernel-gate.sh
```

`boot-direct.sh` exits 0 only when the requested target is met, 2 when the loader/kernel fails
or the target is missed (the output names the measured breakdown and the exact blocker), and 3
for a usage error. A missing or broken loader never produces a fake banner.

## Measured results (OBSERVED)

Host: `qemu-system-x86_64` **6.2.0**, `/dev/kvm` present. The host is shared, so the wall-clock
numbers vary between runs; the medians below are from the quoted runs. `-smp 1` is used for the
KVM rows because KVM with more than one vCPU currently hangs in this tree (see below); that is a
pre-existing kernel SMP/AP issue, not a loader setting.

| Path | Loader start | Loader work | Kernel banner | Release login |
|---|---|---|---|---|
| UEFI/OVMF, TCG (round 1 baseline) | n/a | n/a | median **38.2 s** | median **166 s** |
| Direct, q35, TCG, release initfs (3 runs) | median 211 ms | n/a | median **3.244 s** (p90 4.902 s) | median **8.052 s** (p90 8.522 s) |
| Direct, microvm, KVM, smp=1, release initfs (`boot-direct.sh`, 3 runs) | median 332 ms | median 104 ms | median **1.504 s** (p90 1.841 s) | not run |
| Direct, q35, KVM, smp=1, release initfs (1 run) | 551 ms | 168 ms | **1.589 s** | **2.327 s** |

The loader itself reaches the kernel in one step: the loader-work column is the time from
`direct: loader start` to `direct: entering kernel`. The kernel then runs its normal early init
(frame allocator, paging, heap, initfs CRC, drivers, scheduler, ACPI/APIC, timer). The
`kernel-banner` line is printed only after that init, by the **unchanged** kernel.

The full release image was verified on the direct path: q35 + virtio-net, release initfs, KVM,
`smp=1` reaches `svcd: all services up`, `netd: interface eth0 10.0.2.15/24`, the getty banner
and `login:`. No service is disabled and rsh is not part of the path.

## Why the <0.2 s / <1 s targets are still missed

* QEMU 6.2's `-kernel` PVH path starts an option ROM (`pvh.bin`), not a direct CPU reset into
  the ELF. The measured loader-start column (0.15-0.55 s) is QEMU/SeaBIOS/option-ROM time before
  the loader's first line. A boot path without that stage needs a different QEMU mechanism or a
  minimal firmware; both are outside this loader and would still have to build `BootInfo`.
* Under TCG the unchanged kernel's early init (dominated by the 41 MB release-initfs CRC and
  SMP/AP and timer bring-up) takes seconds. Under KVM it is much smaller, but KVM with `-smp 4`
  hangs after `timer: 1000 Hz` in this tree; the same hang happens with the UEFI image, so it is
  a pre-existing kernel/SMP issue (S83), not the direct loader. `-smp 1` is reliable.
* The full <1 s login gate additionally depends on S84/S95/S96 (service/network/brush work); the
  direct path now boots the same release userland, but the gate is not closed by this ticket.

The exact remaining loader-side work is therefore **none**: the loader builds the handoff and
enters `_start`. The remaining time is QEMU's PVH option-ROM stage plus kernel early init, and
the KVM SMP hang is kernel-owned.

## Kernel entry contract (unchanged, now satisfied by the loader)

`kernel/src/arch/x86_64/start.rs` `_start` requires long mode, paging on, interrupts off, a
valid stack and `rdi` = HHDM virtual pointer to `raven_bootinfo::BootInfo`. The direct loader
provides all of these. The loader also fills the required fields (`magic`, `version`, `size`,
`arch`, `hhdm_offset`, kernel bases/size, memory map, initfs, cmdline, `acpi_rsdp`, boot CPU,
TSC counter, framebuffer = none).

The previous round's PVH note warning is resolved by the loader's own `XEN_ELFNOTE_PHYS32_ENTRY`
note; the kernel ELF still has no PVH note and is not edited.

## Files

* `boot/direct/` (`Cargo.toml`, `loader.ld`, `src/entry.S`, `src/lib.rs`, `src/lib_ut.rs`,
  `src/main.rs`)
* `scripts/boot-direct.sh`
* this note
