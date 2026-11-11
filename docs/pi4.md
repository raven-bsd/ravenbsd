<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Raspberry Pi 4 boot path (`kernel8.img`)

Status: S15a implementation. QEMU `virt` evidence is OBSERVED on the development host; the board
run is UNVERIFIED (no Pi 4 attached). S15b adds the Pi-specific interrupt controller, timer
routing and SMP bring-up.

## 1. What the firmware does

The Pi 4 has no UEFI. The VideoCore firmware loads a flat `kernel8.img` at physical `0x80000`,
enters the first instruction at EL2, and passes the physical address of the board device tree in
`x0`. `kernel/linker/aarch64-pi4.ld` links the low entry section at `0x80000` and the kernel at its
usual higher-half virtual base. `kernel/src/arch/aarch64/pi4/entry.rs` parks secondary cores,
drops EL2 to EL1, zeroes both BSS regions and calls the Rust low-level builder
(`kernel/src/arch/aarch64/pi4/early.rs`).

The builder uses a static 64-frame pool in `.bss`, identity-maps the stub and its early stack,
maps a RAM window plus the kernel segments into the higher half, maps the PL011/GIC device windows
through the HHDM, enables the MMU, and jumps to `pi4_main`. `pi4_main` parses the FDT, builds a
`raven_bootinfo::BootInfo`, selects the PL011, and calls the same `kmain` used by the UEFI path.

## 2. Build

```
RAVEN_PI4=1 cargo xtask build kernel --arch aarch64 --release
llvm-objcopy -O binary build/aarch64/kernel.elf build/aarch64/kernel8.img
```

`RAVEN_PI4=1` selects `linker/aarch64-pi4.ld` and enables the Pi entry module; without it the
normal UEFI entry is built. The ELF stays at `build/aarch64/kernel.elf`.

### Optional initfs

The kernel accepts an initfs appended directly after the loaded image (the probe reads
`__image_mem_end`). Pad the flat image to the image's memory end first, then append the archive:

```
cargo xtask initfs --arch aarch64
size=$(llvm-nm build/aarch64/kernel.elf | awk '/__image_mem_end/{print $1}')
truncate -s $((0x$size - 0x80000)) build/aarch64/kernel8.img
cat build/aarch64/initfs.img >> build/aarch64/kernel8.img
```

Without an appended initfs the kernel prints the banner and halts at `boot: early init complete,
halting`; with one it starts `/init`.

## 3. SD card contents

Use a FAT32 boot partition. Copy these files from
`ravenbsd-resources/rpi-firmware/boot/` (the blobs are proprietary and are never committed):

* `start4.elf`
* `fixup4.dat`
* `bcm2711-rpi-4-b.dtb`
* `kernel8.img` (built above)

`config.txt`:

```
arm_64bit=1
kernel=kernel8.img
enable_uart=1
dtoverlay=disable-bt
init_uart_clock=48000000
core_freq=250
```

`enable_uart=1` routes the console to GPIO14/15. `dtoverlay=disable-bt` keeps the PL011 on
`0xFE20_1000` instead of handing it to Bluetooth. `init_uart_clock=48000000` is the clock the
kernel assumes for the Pi (`Platform::uart_clock_hz`); if the firmware reports a different clock,
set it here to match.

Put the card in the Pi, connect the UART, and power on.

## 4. UART wiring and capture

The Pi 4 header exposes the PL011 on GPIO14/15 at 3.3 V:

| Pi pin | Signal | USB-UART adapter |
|---|---|---|
| 6 | GND | GND |
| 8 | GPIO14 / TXD | RXD |
| 10 | GPIO15 / RXD | TXD |

Do not connect the adapter's VCC to the Pi. Use 115200 8N1, no flow control. Capture with:

```
picocom -b 115200 -d 8 -p n -s 1 /dev/ttyUSB0
# or
screen /dev/ttyUSB0 115200
```

Expected output starts with the firmware and then:

```
RavenBSD kernel 0.1 (aarch64) <build id>
board: Raspberry Pi 4 Model B Rev 1.x
mem: ... MiB usable in ... regions
```

## 5. QEMU smoke tests (development host)

QEMU 6.2.0 on the development host has no `raspi4b` machine (it lists only `raspi0/1/2/3`).
`-M raspi4b` needs QEMU >= 9.0; that attempt was NOT RUN here. The Pi entry path can still be
exercised on `-M virt`, both with EL1 entry and with the EL2 path the firmware uses:

```
# EL1 entry (QEMU's default for -kernel)
qemu-system-aarch64 -M virt,gic-version=2 -cpu cortex-a72 -m 512M -nographic \
  -no-reboot -kernel build/aarch64/kernel8.img -dtb virt.dtb

# EL2 entry, exercising the HCR_EL2/SPSR_EL2/ELR_EL2 drop
qemu-system-aarch64 -M virt,virtualization=on,gic-version=2 -cpu cortex-a72 -m 512M \
  -nographic -no-reboot -kernel build/aarch64/kernel8.img -dtb virt.dtb
```

A DTB for `-dtb` can be produced with:

```
qemu-system-aarch64 -M virt,dumpdtb=virt.dtb -cpu cortex-a72 -m 512M -nographic
```

Both runs OBSERVED: the FDT is parsed, the banner prints, the generic timer and the heap come up,
and with an appended initfs the kernel starts `/init` and runs it to `init exited (0)`. The board
model line is absent on `virt` because that DTB is not a Raspberry Pi tree.

## 6. Implementation notes

* `crates/raven-bootinfo-fdt` converts an FDT into a `BootInfo` and a platform description, and
  builds the flat-entry page tables. It is `no_std`, allocation-free and host-tested; the kernel
  compiles the same sources with `#[path]` so the host tests exercise the shipped code.
* `crates/raven-fdt-lite` gained `children()` and `reservations()`; both are bounds-checked and
  covered by host tests.
* Console selection: the FDT `arm,pl011`/`arm,pl011-axi` node is translated through the parent
  `ranges` (the Pi `soc` node maps `0x7e20_1000` to `0xfe20_1000`). The clock is 48 MHz on a
  Raspberry Pi and 24 MHz on QEMU `virt`.
* The RAM window mapped before the MMU is 2 GiB from physical 0 on the Pi and 1 GiB from
  `0x4000_0000` on QEMU `virt`. The kernel installs its own tables from the FDT memory map after
  the handoff; S15b owns high-RAM and SMP details.
* The boot path was written from the architectural requirements and the `rpi-firmware` DTB. The
  `raspberrypi-os-tutorials` checkouts were read for the EL2 drop and PL011 register sequence;
  no source file was copied. The firmware blobs and DTB are not part of this repository.
