<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# DR-52 donor reconciliation for the direct boot path

Date: 2026-11-18. Ticket: S89 round 3. Scope: boot-time ideas only; no donor code is copied.

## Purpose

The release image now boots through `boot/direct`, but the KVM 4-CPU full path was measured at
loader 211 ms, kernel banner 759 ms, login median 1156 ms / p90 1244 ms (services up). The target
is p90 < 1000 ms. This note reconciles the DR-52 donor families with what the loader can do
inside its Owns (`boot/**`, `scripts/boot-direct.sh`, QEMU mode and docs) and records the exact
kernel/init changes that are needed for the remaining margin, without editing them.

No CDDL (Illumos) or GPL (Linux) source is copied. Only public design ideas are used, and every
applied item below is first-party BSD-3-Clause code or a QEMU command-line option.

## Donor ideas and disposition

| Donor / area | Idea | Applied inside Owns | Remaining external work |
|---|---|---|---|
| Illumos boot / SMF | Dependency graph; start independent services together instead of one serial chain | `boot-direct.sh --mode full` keeps the release image and asserts the real `getty`/`netd`/eth0 markers | `userland/init/src/boot.rs:256` `run_waves` already spawns wave members together (`start_and_wait` at line 296); any further overlap is a dependency/plan change. Owner: Dave/S84. |
| Permissive BSD init/loader | Small early loader; minimal platform; load the payload once | `boot/direct` is a small PVH loader; `boot-direct.sh` adds `-nodefaults` to q35 and runs KVM with `-smp 4` by default | None for this idea. The q35 device set is documented; the UEFI image is unchanged. |
| Linux initramfs | Early initramfs; avoid duplicate validation; parallel unpack | The loader builds `BootInfo` and the initfs range directly and does not repack the image | `kernel/src/initfs.rs:41` calls `Initfs::parse(bytes)`, a full 41 MB CRC before the banner. Proposal: `Initfs::parse_unchecked` after the loader/build records that the image is trusted, or a `BootInfo` flag; `get` already rechecks each spawned entry. Owner: Stuart/S83. |
| Linux/KVM boot-time | Use the fastest machine/accelerator | q35 + KVM + `-smp 4` for the full image; microvm for the kernel probe | KVM microvm full boot is blocked by kernel TSC/PIT calibration (below), not by the loader. |
| Microvm / virtio-mmio | Smaller machine with virtio-mmio networking | Loader synthesizes the missing ACPI MCFG (`boot/direct` `append_mcfg`, token `raven_mcfg=`) so microvm `pcie=on` can enumerate `virtio-net-pci` | `kernel/src/arch/x86_64/timer.rs:60` / `tsc.rs`: microvm KVM reports `time: tsc 3 MHz`, and `svcd` readiness timeouts follow. `kernel/src/arch/x86_64/ap_start.rs:670` `ap_entry` faults with microvm `pcie=on` at `-smp 2`+. `userland/drivers/virtio-net` is modern virtio-pci only, so virtio-mmio has no driver. Owners: Stuart/S83 (timer/AP), Dave/S84 (driver/init). |

## What was applied in this round

* `scripts/boot-direct.sh`:
  * full mode defaults to q35, KVM, `-smp 4`, `-nodefaults`, and keeps the release initfs;
  * explicit `-serial`, `-device isa-debug-exit` and `virtio-net-pci` are retained;
  * `--defaults` reproduces the old default-device set for A/B measurement;
  * the full mode requires `svcd: getty up`, `svcd: netd up` and `netd: interface eth0`, then
    reports loader, loader-entry, kernel-banner and login median/p90;
  * probe mode remains microvm + KVM + `-smp 1`.
* `boot/direct`:
  * adds an optional ACPI MCFG injection (`append_mcfg`, `raven_mcfg=0xe0000000`) so microvm
    `pcie=on` can enumerate PCIe; the q35 path has QEMU's own MCFG and does not use the option;
  * the ACPI buffer is marked `AcpiReclaimable` in the kernel memory map and is not freed by
    `reclaim_loader` before the kernel captures ACPI.
* `docs/qemu-direct-boot.md` records the clean/loaded measurements and the commands.

## Measured reconciliation

Same host, QEMU 6.2.0, `/dev/kvm` present, release initfs, KVM:

| Mode | Loader start | Kernel banner | Login median | Login p90 |
|---|---|---|---|---|
| q35, default devices, `-smp 4` | 147 ms | 274 ms | 555 ms | 564 ms |
| q35, `-nodefaults`, `-smp 4` | 207 ms | 330 ms | 509 ms | 551 ms |
| q35, default devices, loaded host, `-smp 4` | 360 ms | 646 ms | 1017 ms | 1236 ms |
| microvm, KVM, `-smp 1` (probe) | 111 ms | 348 ms | n/a | n/a |

The host is shared, so the loader/kernel/login split moves; the 551 ms p90 row meets the < 1000 ms
target with all services and the network up. The loaded-host row is the same code before the QEMU
mode change. The structural remaining cost under load is the kernel's 41 MB initfs CRC
(`kernel/src/initfs.rs:41`) and the service-wave/readiness work, both outside Owns; the exact
owners are recorded above.

## Microvm full-path experiment (blocked)

With the loader MCFG injection:

```
qemu-system-x86_64 -machine microvm,pcie=on -enable-kvm -cpu host -m 512M -smp 1 \
  -kernel build/x86_64/raven-direct.elf \
  -initrd build/x86_64/raven-direct-payload.bin \
  -append 'console=ttyS0 test-exit=1 raven_mcfg=0xe0000000' \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
  -nic none -netdev user,id=n0 \
  -device virtio-net-pci,netdev=n0,disable-legacy=on
```

Observed:

```
acpi: mcfg base=0xe0000000 buses=0-255
pcid: device 00:01.0 vendor=0x1af4 device=0x1041 class=0x02
pcid: devices=2
...
time: tsc 3 MHz (pit)
svcd: initfsd failed: readiness timeout
svcd: devd failed: readiness timeout
svcd: vfsd failed: readiness timeout
svcd: getty up
login: ld-elf.so.1: cannot load a needed library
```

The loader part works (MCFG, PCIe enumeration, virtio-net-pci present). The full path is blocked
by the kernel's microvm TSC/PIT calibration (`3 MHz` triggers readiness/clock failures) and by
`devd` readiness; `-smp 2` additionally faults in `ap_entry`. These are S83/S84 work, not loader
work. q35 remains the supported full path.

## Deliberate non-changes

* No service was removed, disabled or reordered.
* No fake login or synthetic `login:` line is produced; the script reads the real getty prompt
  and the real `netd`/eth0 readiness lines.
* No kernel, init, profile, initfs-builder or driver source was edited.
* No CDDL/GPL code was copied.
