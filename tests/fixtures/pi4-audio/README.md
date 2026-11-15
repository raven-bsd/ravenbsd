<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 PWM audio DTB fixture (B46)

This is an **original minimal device tree**, not a copy of a donor DTB. It contains the nodes and
values `raven-pwm` needs to agree with the live Pi 4 tree: PWM1 at `0x7e20c800` (CPU
`0xfe20c800`), the GPIO block and its `audio_pins` group for GPIO40/41 ALT0 pull-off, and CPRMAN
at `0x7e101000` (CPU `0xfe101000`).

The `.dtb` is a generated intermediate. The committed test input is `pi4-audio.dtb.hex`, which
the repository's provenance checker can read as text; `raven-pwm/src/dtb_ut.rs` decodes it back
to the same FDT bytes.

## Regeneration

```
python3 tests/fixtures/pi4-audio/generate.py
```

The script writes `pi4-audio.dtb` (local inspection only), `pi4-audio.dtb.hex` (test input) and
`pi4-audio.dts` (documentation). It uses only the Python standard library; `dtc` is not required.
Delete the generated `.dtb` after inspection so the tree stays text-only.

## Sources of the values

| Source | Revision | Licence | What was read |
|---|---|---|---|
| `tickets/bob/reports/B41.md` | directive rev 37 | project report | Live-DTB observations: PWM1 `reg=<0x7e20c800 0x28>`, `audio_pins` pins 40/41 ALT0 pull-off, CPRMAN and GPIO bases, DMA masks. |
| `ravenbsd-resources/netbsd/sys/external/gpl2/dts/dist/arch/arm/boot/dts/broadcom/bcm2711.dtsi` | `36eddebcc36cb0775aa2962005b9c05419a6ff01` | `GPL-2.0 OR MIT` | `pwm1: pwm@7e20c800` compatible/reg, `gpio@7e200000` reg, `pwm1_0_gpio40` and `pwm1_1_gpio41` ALT0/bias-disable groups. |
| `ravenbsd-resources/netbsd/sys/external/gpl2/dts/dist/arch/arm/boot/dts/broadcom/bcm283x.dtsi` | same | `GPL-2.0 OR MIT` | CPRMAN `reg = <0x7e101000 0x2000>` and `#clock-cells = <1>`. |
| `ravenbsd-resources/rpi-firmware/boot/bcm2711-rpi-4-b.dtb` | `ab5c7f86283ceb58b7adb269cae1d16092a31bf6` (tier C) | Broadcom firmware package | `/soc` `ranges`, the live `audio_pins` property values, and the PWM1 node status. Only data values were read; the DTB is not committed. |

No donor source is copied. The node names and property values are facts required by the
device-tree binding; the tree itself is written for this fixture.

## Decoded values

| Item | Child address | CPU address | Size | Node |
|---|---|---|---|---|
| PWM1 | `0x7e20_c800` | `0xfe20_c800` | `0x28` | `/soc/pwm@7e20c800` |
| GPIO | `0x7e20_0000` | `0xfe20_0000` | `0xb4` | `/soc/gpio@7e200000` |
| CPRMAN | `0x7e10_1000` | `0xfe10_1000` | `0x2000` | `/soc/cprman@7e101000` |
| Audio pins | - | - | - | `/soc/gpio@7e200000/audio_pins` |

The audio pin group has `brcm,pins = <40 41>`, `brcm,function = <4>` (ALT0) and `brcm,pull = <0>`
(pull-off), matching the B41 live-DTB observation.

The `/soc` range is `<0x7e000000 0x0 0xfe000000 0x01800000>`, the Pi 4 low-peripheral window.

## Use

`crates/raven-pwm/src/dtb_ut.rs` asserts:

* the PWM1 node has compatible `brcm,bcm2835-pwm` and translates to `PWM1_BASE`/`PWM1_SIZE`;
* the GPIO node translates to `GPIO_BASE` and carries the two audio pins with ALT0/pull-off;
* the CPRMAN node translates to `CPRMAN_BASE`.

The fixture describes the expected firmware tree; it is not evidence that a board was measured.
Board reads remain UNVERIFIED until a log exists (see `docs/pi4-audio.md`).
