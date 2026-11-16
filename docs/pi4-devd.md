<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 platform discovery and driver launch in `devd` (K32, P8)

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
controller with `#interrupt-cells = <3>`. `GicInterrupt::intid` converts the tree number to the
GIC INTID the kernel expects (SPI numbers start at 32, PPI numbers at 16).

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

## `devd dtb` modes

`devd dtb` is the K32 discovery mode. It reads `/etc/pi4.dtb` from the initfs, prints every decoded
device and every compatible match, then exits. It creates no handles:

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

`devd dtb launch` is the P8 production path. It selects each matched driver's resources, creates
the declared handles, spawns the driver with the PCI token shape, and waits for every child in
match order:

```
devd: dtb driver fake-platform compatible=brcm,bcm2711-genet-v5 device=/scb/ethernet@7d580000
devd: dtb launch fake-platform bar=0 size=0x10000 dma=1048576 irq=189
fake-platform: bar=0 size=65536 dma=1048576 irq=189
fake-platform: dma bus=0x40555000 len=1048576
fake-platform: handles ok
devd: dtb launch fake-platform exit=0
devd: dtb matched=1
```

All matched drivers are spawned before the first `PROC_WAIT`, so a long-lived driver such as the
GENET `nic:` server cannot starve the drivers after it. If a later spawn fails, the already
spawned children are terminated and reaped before `devd` reports the error.

## Resource selection and grants

The manifest `args` use the PCI selectors:

* `bar=N` selects the device's `reg` window (default `0`).
* `resources=dma+irq` asks for a DMA buffer and the device's first GIC interrupt. `resources=none`
  clears both; bare `dma` and `irq` tokens are also accepted.
* `dma_size=N` sets the DMA length (default 1 MiB; rounded up to the 4096-byte platform page).
* Every other token is forwarded to the driver before the generated tokens.

`platform::plan` is pure and validates the whole request before `devd` creates any handle. A
missing `reg` index, a missing interrupt, a zero DMA length or an unknown selector returns an
error and no grants. `platform::PlatformPlan::grants` then fixes the transfer order:

| Slot | Grant | Created with |
|---|---|---|
| 3 | selected `reg` window, page-rounded | `mmio_create(root, base, len, 0)` |
| 4 | requested DMA buffer, page-rounded | `dma_alloc(root, len, 4096, 32, 0)` |
| 5 | first GIC interrupt | `irq_create(root, IRQ_LEGACY, intid, None)` |

Only the requested entries are present, so a driver that asks for no DMA has its interrupt at
slot 4. A fresh child has handles 0 (self), 1 (initfs) and 2 (namespace). The PCI path also
transfers a configuration page, so its slot numbers differ; the argv token shape is the same.

The generated tokens are `bar=<n>`, `bar_size=<bytes>`, and `dma=<actual-len>` / `irq=<intid>` when
requested. `dma` comes from `dma_info` so a rounded allocation is reported accurately. Handles are
closed on every creation or spawn failure; on a successful spawn they have moved to the child, so
`devd` does not close the stale source values.

## Readiness contract

The GENET driver serves the B39 `nic:` frame service. The platform service entry for `devd` must
declare the child's scheme as its readiness gate:

```toml
name = "devd"
exec = "/sbin/devd"
args = ["devd", "dtb", "launch"]
provides = ["scheme:nic", "scheme:dev"]
readiness = "scheme"
ready_timeout_ms = 60_000
```

`netd` then declares `needs = ["scheme:nic"]` (see `rootfs/server/etc/services.d/netd.toml` for the
QEMU virtio-net entry). `svcd` starts `netd` as soon as `nic:` exists, even while `devd` is still
waiting for the long-lived GENET child. This mirrors `profiles/server.d/devd.manifest`, where the
child driver registers `nic:` before `devd` finishes its device pass. The REL3 platform profile
owns the actual `/sbin/genet` program and its service entry; P8 does not edit `profiles/pi4*`.

## Fixtures and qtest

`tests/fixtures/pi4-devd/` contains `pi4-devd.dtb.hex`/`.dts` (the four peripheral nodes, a GIC,
the `soc`/`scb`/`emmc2bus` ranges and the EMMC2 DMA window) and
`pi4-devd-bad-ranges.dtb.hex`, whose `emmc2bus` range no longer covers the MMC node. Regenerate
both with:

```
python3 tests/fixtures/pi4-devd/generate.py
```

`tests/fixtures/pi4-devd/driver/` is the fake platform driver. It matches the GENET compatible
string, prints the tokens, verifies the BAR/DMA/IRQ handles at slots 3/4/5 and closes them without
touching device memory. `sh tests/fixtures/pi4-devd/build.sh` decodes the good fixture, builds
`raven-init`, `raven-devd` and the fake driver for aarch64, and packs the fixture initfs. The
qtest's `pre` step runs it. The initfs installs `/etc/pi4.dtb`, a test `drivers.toml` that maps
GENET to `/sbin/fake-platform`, a one-shot `devd` service (`devd dtb launch`) and `svcd.conf`.

`tests/qtest/cases/pi4-devd.toml` runs the fixture on aarch64 and asserts the decoded devices, the
launch tokens, the transferred DMA and IRQ handles, the clean driver exit, and `init exited (0)`:

```
cargo xtask qtest --case pi4-devd --arch aarch64
```

The case is aarch64 because the fixture is a Pi 4 tree and the interrupt is a GIC INTID (189),
which has no x86_64 legacy-IOAPIC equivalent. The fixture runs `devd` under the `pcid` service
name because init's root-device grant is reserved for `pcid`; the production Pi 4 profile still
needs the REL3 broker/root policy. The K32 `profiles/pi4-devd` profile remains discovery-only
(`devd dtb`).

## Limits and follow-ups

* REL3 stages `/sbin/genet`, its service entry and the Pi 4 profile fragment. The current
  `userland/drivers/genet/src/main.rs` still reads the PCI handle layout (BAR 3, DMA 5, IRQ 6);
  it must move to the platform layout (BAR 3, DMA 4, IRQ 5) before `devd dtb launch` can use it.
  `emmc2` already uses the platform order, but its argv contract is not yet the generated
  `bar`/`bar_size`/`dma`/`irq` shape.
* There is no board claim. The qtest proves the DTB match, the grant handoff, the token shape and
  clean teardown against the fixture, not GENET registers or the Pi 4 hardware.
* The production service entry must declare `provides = ["scheme:nic", "scheme:dev"]` and
  `readiness = "scheme"` as described above; devd itself does not register `dev:` while it waits
  for a long-lived platform driver.
