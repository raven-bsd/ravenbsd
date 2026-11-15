<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 GENET DTB fixtures (O1)

These fixtures are **original minimal device trees**, not copies of a donor DTB. They contain the
nodes and values the `raven-genet` parser needs. The `.dtb` files are generated; edit
`generate.py`, not the binaries.

## Regeneration

```
python3 tests/fixtures/pi4-genet/generate.py
```

The script writes the binary `.dtb`, the committed hex form `.dtb.hex` (the test input) and the
readable `.dts` (documentation). It uses only the Python standard library; `dtc` is not
required. The binary `.dtb` is generated for local inspection and is not committed: the
repository's provenance checker requires every tracked file to be readable text, and O1 does not
own the root `.provenance-ignore`. The tests decode `.dtb.hex` into the same FDT bytes.

## Sources of the values

| Source | Revision | Licence | What was read |
|---|---|---|---|
| `ravenbsd-resources/rpi-firmware/boot/bcm2711-rpi-4-b.dtb` | `ab5c7f86283ceb58b7adb269cae1d16092a31bf6` (tier C) | Broadcom firmware package | `/scb` `ranges`/`dma-ranges`, `ethernet@7d580000` `reg`/`interrupts`/`phy-mode`, `mdio@e14` `reg`, `ethernet-phy@1` `reg`. The DTB is not committed or redistributed. |
| `ravenbsd-resources/netbsd/sys/external/gpl2/dts/dist/arch/arm/boot/dts/broadcom/bcm2711.dtsi` | `36eddebcc36cb0775aa2962005b9c05419a6ff01` (tier A checkout; the DTS file says `GPL-2.0 OR MIT`) | `GPL-2.0 OR MIT` | GENET/MDIO node shape and the `/scb` bus cell counts. No source copied. |
| `ravenbsd-resources/netbsd/sys/external/gpl2/dts/dist/arch/arm/boot/dts/broadcom/bcm2711-rpi-4-b.dts` | same | `GPL-2.0 OR MIT` | `phy-mode = "rgmii-rxid"`, PHY address 1 and the `status = "okay"` override. No source copied. |

The `rpi-firmware` donor is tier C (proprietary Broadcom licence). Only the DTB **data values**
were read for the fixture; the firmware blob and the DTB binary are not in this repository.

## Decoded values

All addresses are CPU physical addresses after the parent `ranges` translation. The node unit
addresses in parentheses are the pre-translation child addresses.

| Item | Value | Source node |
|---|---|---|
| Root compatible | `raspberrypi,4-model-b`, `brcm,bcm2711` | `/` |
| Model | `Raspberry Pi 4 Model B` | `/` |
| Root cells | `#address-cells = 2`, `#size-cells = 2` | `/` |
| `/scb` range | child `0x0_7c00_0000` -> CPU `0x0_fc00_0000`, size `0x0380_0000` | `/scb` |
| `/scb` range 2 | child `0x6_0000_0000` -> CPU `0x6_0000_0000`, size `0x4000_0000` | `/scb` |
| `/scb` DMA range | bus `0x4_7c00_0000` -> CPU `0x0_fc00_0000`, size `0x0380_0000` | `/scb` |
| `/scb` DMA range 2 | bus `0x0_0000_0000` -> CPU `0x0_0000_0000`, size `0x4_0000_0000` | `/scb` |
| GENET | `0xfd58_0000` + `0x1_0000` (node `0x7d58_0000`) | `/scb/ethernet@7d580000` |
| GENET interrupts | GIC SPI 157 and 158, level high | same node |
| MDIO | offset `0xe14`, size `0x8`; CPU `0xfd58_0e14` | `/scb/ethernet@7d580000/mdio@e14` |
| PHY | address 1, `rgmii-rxid` (valid), `rgmii-id` (no-mac) | `/scb/.../mdio@e14/ethernet-phy@1` |
| MAC | `02:00:00:00:00:42` (synthetic; valid only) | `local-mac-address` |
| Memory fixture | `0x0..0x4000_0000` (1 GiB) | `/memory@0` |

The synthetic MAC is locally administered and is not a hardware address read from a donor.

## Fixtures

| File | Expected parse |
|---|---|
| `pi4-genet-valid.dtb.hex` | Valid tree: `base=0xfd58_0000`, MDIO `0xe14`, SPI 157/158, PHY 1, `rgmii-rxid`, MAC present, two DMA windows. |
| `pi4-genet-no-mac.dtb.hex` | Same tree without `local-mac-address`; `mac == None`, `phy-mode = "rgmii-id"`. |
| `pi4-genet-bad-mdio.dtb.hex` | `mdio@e14` `reg` has one cell instead of the parent's two; parser returns `BadMdio`. |

The invalid fixture uses a synthetic malformed cell count. It does not invent hardware addresses;
every address comes from the table above.
