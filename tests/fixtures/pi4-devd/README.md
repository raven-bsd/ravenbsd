<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# pi4-devd fixtures

Minimal Raspberry Pi 4 device trees for K32 (`devd` platform discovery) and the P8 platform
launch qtest. The trees are our own constructions; only numeric facts and compatible strings from
the Pi 4 firmware DTB and the B42 spike are transcribed. No donor DTS text is copied.

| File | Purpose |
|---|---|
| `pi4-devd.dtb.hex` | Four peripheral nodes (GENET, EMMC2, PWM1, RNG200), a GIC interrupt controller, the `soc`/`scb`/`emmc2bus` ranges and the EMMC2 DMA window. |
| `pi4-devd-bad-ranges.dtb.hex` | Same tree with an `emmc2bus` range too small for the MMC node; `dtb::parse` returns `UnmappedReg`. |
| `generate.py` | Writes both `.dtb.hex` files and their `.dts` sources. |
| `build.sh` | Decodes `pi4-devd.dtb.hex`, builds `raven-init`, `raven-devd` and the fake platform driver for aarch64, and packs `build/fixtures/pi4-devd/initfs.img`. |
| `driver/` | `pi4-devd-fake-driver`: prints the launch tokens, verifies the BAR/DMA/IRQ handles and exits without touching device memory. |
| `etc/` | Test `drivers.toml` (GENET -> `/sbin/fake-platform`), the one-shot `devd dtb launch` service and `svcd.conf`. |

Regenerate from the repository root:

```
python3 tests/fixtures/pi4-devd/generate.py
```

The `.dts` files are documentation; `dtb_ut.rs` decodes the `.dtb.hex` fixtures. The qtest's
`pre` step runs `build.sh` and boots the resulting initfs on aarch64:

```
cargo xtask qtest --case pi4-devd --arch aarch64
```

The K32 `profiles/pi4-devd` profile stays discovery-only (`devd dtb`); the launch fixture uses its
own initfs so it can install the fake driver and the root-device grant without changing the
production Pi 4 profile.
