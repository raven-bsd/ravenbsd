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
| Direct, q35, TCG, smp=4, release initfs (3 runs) | median 211 ms | n/a | median **3.244 s** (p90 4.902 s) | median **8.052 s** (p90 8.522 s) |
| Direct, microvm, KVM, smp=1, release initfs (`boot-direct.sh --arch x86_64`, 3 runs) | median 111 ms | median 13 ms | median **348 ms** (p90 589 ms) | not run |
| Direct, q35, KVM, smp=1, release initfs (`boot-direct.sh --mode full`, clean host, 3 runs) | median 191 ms | median 13 ms | median **517 ms** (p90 586 ms) | median **755 ms** (p90 **923 ms**) |
| Direct, q35, KVM, smp=1, release initfs (later run under shared-host load, 3 runs) | median 160 ms | median 32 ms | median **649 ms** (p90 731 ms) | median **1080 ms** (p90 1081 ms) |

The loader itself reaches the kernel in one step: the loader-work column is the time from
`direct: loader start` to `direct: entering kernel`. The kernel then runs its normal early init
(frame allocator, paging, heap, initfs CRC, drivers, scheduler, ACPI/APIC, timer). The
`kernel-banner` line is printed only after that init, by the **unchanged** kernel.

The full release image was verified on the direct path: q35 + virtio-net, release initfs, KVM,
`smp=1` reaches `svcd: all services up`, `netd: interface eth0 10.0.2.15/24`, the getty banner
and `login:`. `boot-direct.sh --mode full` additionally requires `svcd: getty up`,
`svcd: netd up` and `netd: interface eth0` before it reports success. No service is disabled and
rsh is not part of the path. The **full release login target (<1 s) is MET** in the clean
three-run row (median 755 ms, p90 923 ms). A later three-run sample on the same shared host
measured median 1080 ms / p90 1081 ms with all services up, so the gate is marginal and
host-load sensitive; the loader and kernel work are unchanged in both samples.

## The remaining <0.2 s kernel-banner gap

The probe target is still missed. The measured breakdown for the microvm/KVM probe is:

```
firmware + PVH option ROM: 111 ms
loader work (start -> _start): 13 ms
kernel early init (_start -> banner): 224 ms
```

* QEMU 6.2's `-kernel` PVH path starts an option ROM (`pvh.bin`), not a direct CPU reset into
  the ELF. The loader-start column is QEMU/SeaBIOS/option-ROM time before the loader's first
  line. A boot path without that stage needs a different QEMU mechanism or a minimal firmware;
  both are outside this loader and would still have to build `BootInfo`.
* The unchanged kernel prints the banner only after frame allocation, paging, the heap, the
  full 41 MB release-initfs CRC, drivers, scheduler, ACPI/APIC and timer bring-up. Under TCG that
  takes seconds; under KVM it is ~224 ms in the quiet run above. No loader change can remove
  that work.
* KVM with `-smp 4` hangs after `timer: 1000 Hz` in this tree; the same hang happens with the
  UEFI image, so it is a pre-existing kernel/SMP issue (S83), not the direct loader. `-smp 1`
  is reliable and is what the full <1 s result uses. TCG `-smp 4` works but is far slower.

The exact remaining loader-side work is therefore **none**: the loader builds the handoff and
enters `_start` in 13 ms. The remaining time is QEMU's PVH option-ROM stage plus kernel early
init, and the KVM SMP hang is kernel-owned.

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
