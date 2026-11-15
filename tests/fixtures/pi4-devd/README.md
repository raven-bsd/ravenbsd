# pi4-devd fixtures

Minimal Raspberry Pi 4 device trees for K32 (`devd` platform discovery). The trees are our own
constructions; only numeric facts and compatible strings from the Pi 4 firmware DTB and the B42
spike are transcribed. No donor DTS text is copied.

| File | Purpose |
|---|---|
| `pi4-devd.dtb` | Four peripheral nodes (GENET, EMMC2, PWM1, RNG200), a GIC interrupt controller, the `soc`/`scb`/`emmc2bus` ranges and the EMMC2 DMA window. |
| `pi4-devd-bad-ranges.dtb` | Same tree with an `emmc2bus` range too small for the MMC node; `dtb::parse` returns `UnmappedReg`. |
| `generate.py` | Writes both `.dtb` files and their `.dts` sources. |

Regenerate from the repository root:

```
python3 tests/fixtures/pi4-devd/generate.py
```

The `.dts` files are documentation; the binary `.dtb` files are what `dtb_ut.rs` includes and what
`profiles/pi4-devd.d/10-devd.toml` installs as `/etc/pi4.dtb`.
