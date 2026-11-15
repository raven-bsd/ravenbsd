<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 5 DTB fixtures (B54, extended by R2)

These fixtures are **original minimal device trees**, not copies of a donor DTB. They contain only
the nodes and values the `raven-pi5` parser and the `raven-rp1` MFD model need. The `.dtb` files
are generated; edit `generate.py`, not the binaries. R2 added the `pi5-rp1-mfd*` fixtures; R3 extended
their Ethernet node with the full property set. The B54 fixtures stay byte-identical.

## Regeneration

```
python3 tests/fixtures/pi5/generate.py
```

The script writes both the binary `.dtb` (the test input) and the readable `.dts` (documentation).
It uses only the Python standard library; `dtc` is not required.

## Sources of the values

The values are transcribed facts, not copied source. No donor file is committed, linked or
redistributed.

| Source | Revision | Licence | What was read |
|---|---|---|---|
| `ravenbsd-resources/rpi-firmware/boot/bcm2712-rpi-5-b.dtb` | `ab5c7f86283ceb58b7adb269cae1d16092a31bf6` (tier C) | Broadcom firmware package | Node names, `reg`, `ranges`, `compatible`, `status`, `/chosen`, `/aliases`, and the RP1 MFD nodes (GPIO banks/pins/line names, pinctrl groups, clock rates, PIO/USB/SRAM windows, firmware shmem). The DTB SHA-256 is `03ce4742452ad52e3e8ec2259a0c705b496970e2b75e6e0b4e5446475ea1b926`. |
| `ravenbsd-resources/netbsd/sys/external/gpl2/dts/dist/arch/arm64/boot/dts/broadcom/bcm2712.dtsi` | `36eddebcc36cb0775aa2962005b9c05419a6ff01` (tier A checkout; the DTS file says `GPL-2.0 OR MIT`) | `GPL-2.0 OR MIT` | Node shape, UART/GIC/timer/PCIe/RP1 address cells. No source copied. |
| `ravenbsd-resources/netbsd/.../bcm2712-rpi-5-b.dts` | same | `GPL-2.0 OR MIT` | Root compatible/model, `/chosen stdout-path`, `/aliases`, memory node note. No source copied. |

The `rpi-firmware` donor is tier C (proprietary Broadcom licence). Only the DTB **data values**
were read for the fixture; the firmware blob and the DTB binary are not in this repository. The
fixture trees are our own minimal constructions whose values are documented below. No RP1
firmware is present or redistributed.

## Decoded values

All addresses are CPU physical addresses after the parent `ranges` translation. The node unit
addresses in parentheses are the pre-translation child addresses.

| Item | Value | Source node |
|---|---|---|
| Root compatible | `raspberrypi,5-model-b`, `brcm,bcm2712` | `/` |
| Model | `Raspberry Pi 5` | `/` |
| Root cells | `#address-cells = 2`, `#size-cells = 2` | `/` |
| `/soc` range | child `0x0..0x8000_0000` → CPU `0x10_0000_0000..0x10_8000_0000` | `/soc@107c000000` |
| BCM2712 UART | `0x10_7d00_1000` + `0x200` (node `0x7d00_1000`) | `/soc@107c000000/serial@7d001000` |
| GIC distributor | `0x10_7fff_9000` + `0x1000` (node `0x7fff_9000`) | `/soc@107c000000/interrupt-controller@7fff9000` |
| GIC CPU interface | `0x10_7fff_a000` + `0x2000` (node `0x7fff_a000`) | same GIC node |
| GIC virtual interface | `0x10_7fff_c000` + `0x2000` | same GIC node |
| GIC virtual CPU | `0x10_7fff_e000` + `0x2000` | same GIC node |
| System timer | `0x10_7c00_3000` + `0x1000`, 1 MHz | `/soc@107c000000/timer@7c003000` |
| Generic timer PPI | non-secure physical 14 (kernel normalised 30) | `/timer` |
| Memory fixture | `0x0..0x2800_0000` (640 MiB) | `/memory@0` |
| Reserved ATF | `0x0..0x8_0000`, no-map | `/reserved-memory/atf@0` |
| CMA | 64 MiB (`0x400_0000`), reusable | `/reserved-memory/linux,cma` |
| PCIe controller | `0x10_0012_0000` + `0x9310` (node `0x10_0012_0000`) | `/axi/pcie@1000120000` |
| PCIe 32-bit window | PCI `0x0` → CPU `0x1f_0000_0000`, size `0xffff_fffc` | controller `ranges` |
| PCIe 64-bit window | PCI `0x4_0000_0000` → CPU `0x1c_0000_0000`, size `0x3_0000_0000` | controller `ranges` |
| RP1 window | local `0xc0_4000_0000` → PCI `0x0` → CPU `0x1f_0000_0000`, size `0x41_0000` | `/axi/pcie@1000120000/rp1` |
| RP1 UART | local `0xc0_4003_0000` → CPU `0x1f_0003_0000` + `0x100` (`disabled` in the firmware tree) | `rp1/serial@30000` |
| RP1 mailbox | local `0xc0_4000_8000` → CPU `0x1f_0000_8000` + `0x4000` | `rp1/mailbox@8000` |
| RP1 clocks | local `0xc0_4001_8000` → CPU `0x1f_0001_8000` + `0x1_0038` | `rp1/clocks@18000` |
| RP1 Ethernet | local `0xc0_4010_0000` → CPU `0x1f_0010_0000` + `0x4000` | `rp1/ethernet@100000` |
| Ethernet compatible | `raspberrypi,rp1-gem`, `cdns,macb` | `rp1/ethernet@100000` |
| Ethernet interrupt | id `6`, flags `4` | `rp1/ethernet@100000: interrupts` |
| Ethernet clocks | `pclk` 12, `hclk` 12, `tsu_clk` 29, `tx_clk` 16 | `clocks`, `clock-names` |
| Ethernet PHY | `rgmii-id`, MDIO address 1, power-down flag, `led-modes = 8` | `phy-mode`, `ethernet-phy@1` |
| Ethernet reset | GPIO 32, active-low, duration 5 | `phy-reset-gpios`, `phy-reset-duration` |
| Ethernet MAC | all zero (bootloader placeholder) | `local-mac-address` |
| Ethernet Cadence hints | `aw2w-max-pipe = 8`, `ar2r-max-pipe = 8`, `use-aw2b-fill` | `cdns,*` |
| RP1 GPIO banks | local `0xc0_400d_0000`, `0xc0_400e_0000`, `0xc0_400f_0000`, `0xc000` each | `rp1/gpio@d0000` (3 `reg` windows) |
| RP1 GPIO pins | 54 (`gpio-ranges` count `0x36`) | `rp1/gpio@d0000` |
| RP1 GPIO interrupts | `(0, 4)`, `(1, 4)`, `(2, 4)` | `rp1/gpio@d0000: interrupts` |
| RP1 pinctrl groups | `uart0` on gpio14/15/16/17, `i2c0` on gpio0/1 | `rp1/gpio@d0000` children |
| RP1 clock rates | 13 assigned IDs, 1 MHz to 1.536 GHz | `rp1/clocks@18000: assigned-clock-rates` |
| RP1 PIO | local `0xc0_4017_8000` + `0x20` | `rp1/pio@178000` |
| RP1 USB | local `0xc0_4020_0000`, `0xc0_4030_0000`, `0x10_0000` each | `rp1/usb@200000`, `rp1/usb@300000` |
| RP1 SRAM | local `0xc0_4040_0000` + `0x1_0000` | `rp1/sram@400000` |
| RP1 firmware shmem | local `0xc0_4040_ff00` + `0x100` | `sram@400000/shmem@ff00` |
| RP1 firmware node | `raspberrypi,rp1-firmware`, `simple-mfd`, mailbox + shmem refs | `/rp1_firmware` |
| `/chosen stdout-path` | `serial10:115200n8` | `/chosen` |
| Alias `serial10` | `/soc@107c000000/serial@7d001000` | `/aliases` |
| Alias `serial0` | `/axi/pcie@1000120000/rp1/serial@30000` | `/aliases` |

The firmware DTB's `/memory@0` value is `0x2800_0000` (640 MiB); its DTS comment says the bootloader
fills it in. The actual board memory size is UNVERIFIED until a board log exists.

## Fixtures

| File | Expected parse |
|---|---|
| `pi5-valid.dtb` | Valid full tree: UART, GIC, timer, memory, reserved ATF, PCIe and RP1. |
| `pi5-minimal.dtb` | Valid tree without `/axi`/RP1 and without reserved memory; `pcie == None`. Models the older NetBSD DTS that has no RP1 node. |
| `pi5-uart-rp1.dtb` | Valid tree where `/chosen` selects `serial0` and the RP1 UART is `okay`; the parser must translate it through the PCIe window to `0x1f_0003_0000`. |
| `pi5-memory-overlap.dtb` | Two `/memory` regions overlap; `OverlappingMemory`. The second region is a synthetic overlap inside the documented first region. |
| `pi5-memory-bad-cells.dtb` | `/memory` `reg` is three cells; `BadMemory`. Synthetic malformed shape. |
| `pi5-gic-missing.dtb` | No GIC node; `MissingGic`. |
| `pi5-bad-magic.dtb` | FDT magic zeroed; `Fdt(BadMagic)`. |
| `pi5-rp1-mfd.dtb` | Valid tree with the full RP1 MFD: GPIO block (3 banks, 54 pins, 3 interrupts, pinctrl groups), clocks with assigned rates, PIO/USB/SRAM windows, the full Ethernet node (interrupt, clocks, RGMII PHY, reset GPIO, Cadence hints) and the firmware mailbox + shmem. |
| `pi5-rp1-mfd-bad-pins.dtb` | The same tree with `gpio-ranges` count zero; `raven-rp1` reports `PinCount(0)`. |
| `pi5-rp1-mfd-bad-clock.dtb` | The same tree with one assigned rate missing; `raven-rp1` reports `ClockCount { clocks: 13, rates: 12 }`. |
| `pi5-rp1-mfd-mac.dtb` | The same tree with a non-zero `local-mac-address` (`02:11:22:33:44:55`); the Ethernet model returns it as a real address. |

The invalid fixtures use synthetic overlaps or malformed cell counts. They do not invent hardware
addresses; every address comes from the table above.

## Parser contract

`raven-pi5` uses `raven-fdt-lite` for the FDT header, node walk, property access and the 1–2 cell
`ranges` translation. The RP1 window needs a 3-cell parent address (the PCIe child address space),
which `raven-fdt-lite` deliberately does not translate; `raven-pi5` decodes that one range itself
and matches the PCIe `ranges` entry by its address-space flags. The parser rejects a tree without a
`/memory`, GIC, generic timer or PL011 UART; a Pi 5 tree without PCIe/RP1 is accepted.
