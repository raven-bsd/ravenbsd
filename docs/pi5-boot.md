<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Raspberry Pi 5 boot path (BCM2712 + RP1)

Status: B54 host-side preparation. **No Pi 5 hardware has been run.** Values marked OBSERVED were
read from the official firmware device tree; they are not silicon evidence. Every hardware-dependent
item is UNVERIFIED until a board log exists. B54 does not touch the kernel or loader; B55 owns
PCIe/RP1 and B59 owns the image/evidence kit.

## 1. Boot chain

```text
BCM2712 boot ROM / SPI EEPROM bootloader
        |
        v
FAT boot partition: config.txt + start4.elf/fixup4.dat + DTB + kernel image
        |
        v
VideoCore firmware initialises the SoC, loads the DTB and the kernel
        |
        v
ARM cores enter the kernel with the DTB address in x0 (entry EL UNVERIFIED)
        |
        v
RavenBSD low-level entry: park secondary cores, drop EL2->EL1, parse the FDT
        |
        v
BCM2712 UART10 console; PCIe/RP1 is B55, Ethernet/USB/GPI0 are later tickets
```

* **PROPOSED** direct path: the firmware loads a flat kernel image plus `bcm2712-rpi-5-b.dtb`, as
  the Pi 4 path does. The Pi 5 firmware package also contains an EFI-stub `kernel_2712.img`; the
  UEFI alternative is not selected by B54.
* **UNVERIFIED:** whether the Pi 5 EEPROM bootloader needs `start4.elf`/`fixup4.dat` or
  `bootcode.bin` at all, whether it selects `kernel_2712.img` or `kernel8.img`, the kernel load
  address, and the entry exception level. Do not encode a guessed load address. The host-side
  `ImageLayout` deliberately has `load_address: None`.
* **UNVERIFIED:** whether the boot firmware or the OS loads the RP1 firmware. B55 is written for the
  OS-loads-it model (`architecture/rpi5/04-firmware-and-provenance.md`), using the official blob
  exactly as Linux does. No blob is included here.

## 2. Platform map from the firmware DTB (OBSERVED)

The addresses are CPU physical addresses after the parent `ranges` translation. The node unit
addresses in parentheses are the pre-translation child addresses.

| Item | Value | Node |
|---|---|---|
| BCM2712 UART10 | `0x10_7d00_1000` + `0x200` (`0x7d00_1000`) | `/soc@107c000000/serial@7d001000` |
| GIC distributor | `0x10_7fff_9000` + `0x1000` | `/soc@107c000000/interrupt-controller@7fff9000` |
| GIC CPU interface | `0x10_7fff_a000` + `0x2000` | same GIC node |
| GIC virtual interface | `0x10_7fff_c000` + `0x2000` | same GIC node |
| GIC virtual CPU | `0x10_7fff_e000` + `0x2000` | same GIC node |
| BCM2835 system timer | `0x10_7c00_3000` + `0x1000`, 1 MHz | `/soc@107c000000/timer@7c003000` |
| Generic timer | PPIs 13, 14, 11, 10; non-secure physical 14 (kernel normalised 30) | `/timer` |
| `/soc` range | child `0x0..0x8000_0000` -> CPU `0x10_0000_0000..0x10_8000_0000` | `/soc@107c000000` |
| PCIe controller | `0x10_0012_0000` + `0x9310` | `/axi/pcie@1000120000` |
| RP1 window | local `0xc0_4000_0000` -> PCI `0x0` -> CPU `0x1f_0000_0000`, `0x41_0000` | `/axi/pcie@1000120000/rp1` |

The other two `brcm,bcm2712-pcie` controllers are `status = "disabled"` in the firmware DTB. The
third controller (`0x10_0012_0000`) is `okay` and has the RP1 child.

All of the above are **OBSERVED in the firmware DTB**, not measured on silicon. GIC/timer behaviour,
the PCIe link and the RP1 window remain UNVERIFIED until a board log exists.

## 3. UART debug path

The firmware DTB selects:

```text
/chosen/stdout-path = "serial10:115200n8"
/aliases/serial10    = "/soc@107c000000/serial@7d001000"
```

So the first console is the **BCM2712 UART10 PL011**, not an RP1 UART. The RP1 UARTs
(`/axi/pcie@1000120000/rp1/serial@30000` and its siblings) are `status = "disabled"` in the
firmware tree. The Pi 5 board exposes a 3-pin JST-SH 1.0 mm connector labelled `UART` (OBSERVED in
the NetBSD DTS comment); it is the intended debug header.

Wiring and capture:

* Use a **3.3 V** USB-UART adapter. Do not connect the adapter's VCC to the Pi.
* 115200 8N1, no flow control.
* **UNVERIFIED:** the connector's GND/TX/RX order. Confirm it with a meter and the board
  documentation before connecting; a swapped TX/RX pair is harmless, reversed power is not.
* **UNVERIFIED:** the UART input clock. The firmware DTB does not carry `clock-frequency` for
  `serial@7d001000`; the Pi 4 uses 48 MHz, but the Pi 5 value is not confirmed. The parser exposes
  `Uart::clock_hz = None` rather than guessing. The `profiles/pi5.toml` console name `ttyAMA0` is
  also UNVERIFIED until the first boot.

The UART interrupt is GIC SPI 121 (`interrupts = <0 0x79 4>`); the parser exposes
`Uart::interrupt = Some(121)`. The BCM2835 system timer is a separate 1 MHz MMIO timer; the generic
ARM timer is the architectural fallback.

## 4. Memory map and reserved regions

The firmware DTB has one `/memory@0` region:

```text
reg = <0x0 0x0 0x0 0x2800_0000>     # 640 MiB, with the DTS note "will be filled by the bootloader"
```

The actual board memory size is **UNVERIFIED**; the bootloader may replace the size for a 4 GB or
8 GB board. The parser keeps the value it sees and rejects overlapping `/memory` regions.

Reserved memory in the firmware DTB:

| Region | Range | Flags |
|---|---|---|
| ATF | `0x0..0x8_0000` | `no-map` |
| CMA | 64 MiB (`0x400_0000`), alloc range `0x0..0x4000_0000` | `reusable`, `linux,cma-default` |
| bootloader config/key | zero-sized and `status = "disabled"` in this DTB | `no-map` |

The kernel must not allocate the ATF region. The parser exposes the reserved ranges separately
from the usable `/memory` regions; it does not subtract them automatically (the later boot-info
builder owns that policy).

## 5. PCIe and the RP1 window

The `brcm,bcm2712-pcie` controller at `0x10_0012_0000` has two `ranges` windows:

| PCI flags | PCI address | CPU address | Size |
|---|---|---|---|
| `0x0200_0000` (32-bit non-prefetchable) | `0x0` | `0x1f_0000_0000` | `0xffff_fffc` |
| `0x4300_0000` (64-bit prefetchable) | `0x4_0000_0000` | `0x1c_0000_0000` | `0x3_0000_0000` |

The RP1 child maps its local `0xc0_4000_0000` window to PCI `0x0`, size `0x41_0000`; therefore the
RP1 register window is CPU `0x1f_0000_0000..0x1f_0041_0000`. RP1-local offsets translate directly:

| RP1 node | Local address | CPU address | Status |
|---|---|---|---|
| `mailbox@8000` | `0xc0_4000_8000` | `0x1f_0000_8000` + `0x4000` | `okay` |
| `clocks@18000` | `0xc0_4001_8000` | `0x1f_0001_8000` + `0x1_0038` | `okay` |
| `serial@30000` | `0xc0_4003_0000` | `0x1f_0003_0000` + `0x100` | `disabled` |
| `ethernet@100000` | `0xc0_4010_0000` | `0x1f_0010_0000` + `0x4000` | `okay` |

`raven-fdt-lite` intentionally translates only 1–2 address cells; the RP1 parent is a PCIe node
with three child address cells, so `raven-pi5` decodes that range itself and matches the PCIe
window by its address-space flags. **UNVERIFIED:** PCIe link training, BAR assignment, MSI/MSI-X
and the RP1 enumeration are B55's work.

## 6. Image layout

The FAT32 boot partition for the direct path is proposed as:

```text
config.txt
bcm2712-rpi-5-b.dtb
kernel_2712.img          # or kernel8.img; the firmware's choice is UNVERIFIED
start4.elf
fixup4.dat
overlays/                # only if a later ticket needs one
```

`config.txt` (host-side descriptor; exact keys are **UNVERIFIED** on the Pi 5):

```text
arm_64bit=1
kernel=kernel_2712.img
device_tree=bcm2712-rpi-5-b.dtb
enable_uart=1
uart_2ndstage=1
```

* The firmware package contains both `kernel_2712.img` and `kernel8.img`; the `pi5` layout lists
  both candidates in that order and does not claim which one the EEPROM selects.
* `start4.elf`/`fixup4.dat`/`bootcode.bin` are present in the official package. Which of them the
  Pi 5 EEPROM bootloader requires is UNVERIFIED; the Pi 5 uses an SPI EEPROM bootloader, unlike
  the Pi 4's `bootcode.bin` path.
* The kernel load address and entry EL are UNVERIFIED. The Pi 4 loads at `0x80000`; do not reuse
  that value for the Pi 5 without a board log.
* The proposed kernel command line is `console=ttyAMA0 loglevel=info` (matches
  `profiles/pi5.toml`). The Pi 4-specific `dtoverlay=disable-bt` is **not** copied: the Pi 5
  Bluetooth UART is a separate BCM7271 UART, and the debug UART is UART10.
* No RP1 firmware blob is in the image. B55 records its path, hash and licence before use; the
  official blob is tier C and is not redistributed by this repository.

The host-side `ImageLayout` lives in `crates/raven-pi5/src/image.rs`; the `xtask/src/pi5.rs`
entry skeleton returns it. B54 does not wire the dispatcher because `xtask/src/cli.rs`,
`xtask/src/image.rs` and `xtask/src/lib.rs` are outside this ticket's Owns; B59 owns the image
ticket and can call `describe()`.

## 7. Questions that require hardware (UNVERIFIED)

1. Actual RAM size (the firmware DTB's `0x2800_0000` is a bootloader placeholder).
2. UART10 input clock and the correct kernel console name (`ttyAMA0`?).
3. Debug connector pin order and voltage (3.3 V only; confirm GND/TX/RX).
4. Whether the EEPROM bootloader selects `kernel_2712.img` or `kernel8.img`, and the load address
   and entry EL.
5. Which of `start4.elf`, `fixup4.dat`, `bootcode.bin` and `overlays/` the Pi 5 boot partition
   actually needs.
6. Whether `enable_uart=1` and `uart_2ndstage=1` are the correct Pi 5 firmware keys.
7. RP1 firmware path, hash, licence and loader (boot firmware vs OS; B55).
8. RP1 UART routing and whether any RP1 UART is usable as a console.
9. GIC-400 and generic-timer behaviour on silicon (the DTB claims both; the PPIs are from the
   tree, not measured).
10. PCIe link training, BAR assignment, MSI/MSI-X and RP1 enumeration (B55).
11. Whether the firmware passes the DTB in `x0` and enters at EL2, as the Pi 4 path assumes.

## 8. Provenance

* `architecture/rpi5/00-overview.md`, `01-hardware.md`, `02-bringup-plan.md`,
  `04-firmware-and-provenance.md`, `06-milestone-and-tickets.md` were read for the plan.
* The official firmware DTB
  `ravenbsd-resources/rpi-firmware/boot/bcm2712-rpi-5-b.dtb`
  (revision `ab5c7f86283ceb58b7adb269cae1d16092a31bf6`, tier C, Broadcom firmware package;
  SHA-256 `03ce4742452ad52e3e8ec2259a0c705b496970e2b75e6e0b4e5446475ea1b926`) was decoded for
  the node values. The DTB binary and the firmware blobs are not committed or redistributed.
* The NetBSD/Linux DTS files
  `ravenbsd-resources/netbsd/sys/external/gpl2/dts/dist/arch/arm64/boot/dts/broadcom/bcm2712.dtsi`
  and `bcm2712-rpi-5-b.dts` (NetBSD revision
  `36eddebcc36cb0775aa2962005b9c05419a6ff01`; file licence `GPL-2.0 OR MIT`) were read for the node
  shape and values. No source file, comment or code was copied or translated.
* The fixtures under `tests/fixtures/pi5/` are original minimal trees generated by
  `generate.py`; the values are the transcribed facts in `tests/fixtures/pi5/README.md`.
* No RP1 firmware blob is present. The RP1 firmware policy and the exact licence path are B55
  decisions; the release decision remains the owner's.

## 9. Host checks

```text
cargo test -p raven-pi5
cargo run -q -p xtask -- help
make check-fast
```

`cargo test -p raven-pi5` covers the valid/minimal/RP1-UART trees, memory overlap and malformed
cells, GIC selection, the bad-magic blob and the image-layout/xtask skeleton. The parser is also
exercised against the official DTB during B54 development, but that donor path is not a committed
test dependency.
