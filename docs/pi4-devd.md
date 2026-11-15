# Pi 4 platform discovery in `devd` (K32)

`devd` reads the platform bus from the firmware flattened device tree (FDT) on aarch64. This
replaces hard-coded Pi 4 peripheral addresses in each driver: the tree supplies the `compatible`
string, the CPU physical `reg` window after `ranges`/`dma-ranges` translation, and the GIC SPI/PPI
numbers.

## Parser

`userland/servers/devd/src/dtb.rs` (host tests in `dtb_ut.rs`) exposes:

* `parse(blob) -> Result<Vec<DeviceSpec>, DtbError>` — depth-first walk of every enabled node that
  has both `compatible` and `reg`. `DeviceSpec` carries the absolute node path, all compatible
  strings, translated `RegWindow`s, the DMA windows inherited from the nearest ancestor with
  `dma-ranges`, and decoded `GicInterrupt`s.
* `match_drivers(devices, drivers)` — joins the device list with `/etc/drivers.toml` entries by
  exact `compatible` string. PCI entries (no `compatible`) are ignored.

The address translation uses `raven-fdt-lite`'s `translate_address`, so a `reg` address outside
every parent `ranges` window is `DtbError::UnmappedReg` rather than a silently skipped device.

Interrupts use the GIC three-cell binding `<type number trigger>`: type `0` is an SPI and type `1`
is a PPI. The parser resolves `interrupt-parent` through the parent chain and the target node's
`phandle`, and requires an `arm,gic-400`, `arm,cortex-a15-gic` or `arm,gic-v3` interrupt
controller with `#interrupt-cells = <3>`.

## Driver table

`userland/servers/devd/drivers.toml` is the compatible-keyed platform table:

```toml
[[driver]]
name = "genet"
compatible = "brcm,bcm2711-genet-v5"
exec = "/sbin/genet"
```

`raven-proto-dev` accepts `vendor`/`device` entries, `compatible` entries, or a mix. `devd`'s PCI
matcher skips entries that have `compatible`.

## `devd dtb` mode

The `dtb` argument reads `/etc/pi4.dtb` from the initfs, prints every decoded device and every
compatible match, then exits:

```
$ devd dtb
devd: dtb devices=5
devd: dtb device /soc/pwm@7e20c800 compatible=brcm,bcm2835-pwm reg=0xfe20c800+0x28 dma=0 irq=none
devd: dtb device /soc/rng@7e104000 compatible=brcm,bcm2711-rng200 reg=0xfe104000+0x28 dma=0 irq=none
devd: dtb device /scb/ethernet@7d580000 compatible=brcm,bcm2711-genet-v5 reg=0xfd580000+0x10000 dma=0 irq=spi=157,spi=158
devd: dtb device /emmc2bus/mmc@7e340000 compatible=brcm,bcm2711-emmc2 reg=0xfe340000+0x100 dma=1 irq=spi=126
devd: dtb device /interrupt-controller@ff841000 compatible=arm,gic-400 reg=0xff841000+0x1000 dma=0 irq=none
devd: dtb driver genet compatible=brcm,bcm2711-genet-v5 device=/scb/ethernet@7d580000
devd: dtb driver emmc2 compatible=brcm,bcm2711-emmc2 device=/emmc2bus/mmc@7e340000
devd: dtb driver pwm1 compatible=brcm,bcm2835-pwm device=/soc/pwm@7e20c800
devd: dtb driver rng200 compatible=brcm,bcm2711-rng200 device=/soc/rng@7e104000
devd: dtb matched=4
```

## Fixtures and qtest

`tests/fixtures/pi4-devd/` contains `pi4-devd.dts`/`.dtb` (the four peripheral nodes, a GIC, the
`soc`/`scb`/`emmc2bus` ranges and the EMMC2 DMA window) and `pi4-devd-bad-ranges.dtb`, whose
`emmc2bus` range no longer covers the MMC node. Regenerate both with:

```
python3 tests/fixtures/pi4-devd/generate.py
```

`profiles/pi4-devd.toml` and `profiles/pi4-devd.d/` install the fixture, the driver table and a
one-shot `devd dtb` service. `tests/qtest/cases/pi4-devd.toml` runs that profile on aarch64 and
asserts the five decoded devices and four compatible matches.

## Follow-ups

* Launching a matched platform driver needs the supervisor to grant the MMIO window, DMA buffer
  and GIC interrupt as capabilities (B47/`platd` and the service profile). `devd dtb` stops at
  matching on purpose; `userland/drivers/**` is untouched by K32.
* The real `profiles/pi4*` image does not yet install `drivers.toml` or the firmware tree as
  `/etc/pi4.dtb`; that production-image fragment is a separate profile task.
