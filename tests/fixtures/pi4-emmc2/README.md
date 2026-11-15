<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 EMMC2 DTB fixtures (J1)

These are **original minimal device trees**, not copies of a donor DTB. They contain only the
nodes and values the `raven-pi4` EMMC2 lookup needs. The `.dtb` files are generated; edit
`generate.py`, not the binaries.

## Regeneration

```
python3 tests/fixtures/pi4-emmc2/generate.py
```

The script uses only the Python standard library; `dtc` is not required. It writes both the binary
`.dtb` (the test input) and the readable `.dts` (documentation).

## Sources of the values

The values are transcribed facts, not copied source. No donor file is committed, linked or
redistributed.

| Source | Revision | What was read |
|---|---|---|
| `ravenbsd-resources/rpi-firmware/boot/bcm2711-rpi-4-b.dtb` | `ab5c7f86283ceb58b7adb269cae1d16092a31bf6` (tier C, `donors.lock`) | The `emmc2bus`/`mmc@7e340000` node shape, `reg`, `ranges`, `dma-ranges`, `interrupts` and `compatible`. |
| `tickets/bob/reports/B42.md` sections 1, 2 and 5 | merged `9bbefa3` | The translated base `0xFE34_0000`, the 1 GiB EMMC2 DMA window and the 3.3 V/4-bit/speed constraints. |
| `ravenbsd-resources/netbsd/sys/external/gpl2/dts/dist/arch/arm/boot/dts/broadcom/bcm2711.dtsi` | `36eddebcc36cb0775aa2962005b9c05419a6ff01` (tier A checkout; the file says `GPL-2.0`) | Numeric facts only: the `emmc2bus` `ranges` and `dma-ranges` values and the SPI 126 interrupt. No text or comment is copied. |

The fixture trees are our own minimal constructions. The board run and the live firmware DTB
window are UNVERIFIED until a Pi 4 log exists; the firmware edits `dma-ranges` per SoC revision, so
the driver must read the live tree and must not hard-code the fixture values.

## Decoded values

All addresses are CPU-physical after the parent `ranges` translation.

| Item | Value | Node |
|---|---|---|
| Root compatible | `raspberrypi,4-model-b`, `brcm,bcm2711` | `/` |
| Root cells | `#address-cells = 2`, `#size-cells = 2` | `/` |
| `/soc` range | child `0x7E00_0000..0x7F80_0000` → CPU `0xFE00_0000..` | `/soc` |
| `emmc2bus` range | child `0x0_7E00_0000..0x0_7F80_0000` → CPU `0x0_FE00_0000..` | `/emmc2bus` |
| EMMC2 registers | `0xFE34_0000` + `0x100` (node `0x7E34_0000`) | `/emmc2bus/mmc@7e340000` |
| EMMC2 interrupt | GIC SPI 126, level high (GIC id 158) | same node |
| EMMC2 DMA window | bus `0xC000_0000` → CPU `0x0`, size 1 GiB | `/emmc2bus` `dma-ranges` |

## Fixtures

| File | Expected parse |
|---|---|
| `pi4-emmc2.dtb` | Valid tree: base, SPI and 1 GiB DMA window. |
| `pi4-emmc2-no-emmc.dtb` | The EMMC2 node is absent; `Emmc2FdtError::MissingNode`. |
| `pi4-emmc2-bad-irq.dtb` | The interrupt flags are active-low; `Emmc2FdtError::BadInterrupt`. |

The invalid fixtures alter only the interrupt flags or omit the node; they do not invent addresses.
