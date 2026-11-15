<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 PWM audio (3.5 mm jack)

Status: host-tested driver core and a skeleton entry point. **No board result is claimed.** Every
register value read from a Pi 4 is UNVERIFIED until a log is attached to a report.

The Pi 4 3.5 mm jack is driven by **PWM1**, not the legacy PWM0 block. This document records the
register facts, the host-tested model and the board bring-up procedure for B46.

## 1. Hardware facts

| Item | Value | Notes |
|---|---|---|
| PWM1 CPU base | `0xfe20_c800` | `reg = <0x7e20c800 0x28>` in the live tree |
| PWM1 size | `0x28` | CTL through DAT2 |
| Legacy PWM0 | `0xfe20_c000` | must stay disabled |
| PWM FIFO bus address | `0x7e20_c818` | DMA destination for duty words |
| CPRMAN CPU base | `0xfe10_1000` | `CM_PWMCTL = 0xa0`, `CM_PWMDIV = 0xa4` |
| GPIO CPU base | `0xfe20_0000` | `GPFSEL4 = 0x10`, `GPPUPPDN2 = 0xec` |
| DMA controller | `0xfe00_7000` | 15 channels; channel stride `0x100` |
| Audio pins | GPIO40, GPIO41 | ALT0, pull-off (BCM2711 pull encoding) |
| PWM clock | 100 MHz | PLLD 500 MHz / `DIVI=5`, `DIVF=0` |
| 48 kHz range | `RNG1 = RNG2 = 2083` | 100 MHz / 2083 = 48 007.7 Hz (+160 ppm) |
| 44.1 kHz range | `RNG1 = RNG2 = 2268` | 100 MHz / 2268 = 44 091.7 Hz (-188 ppm) |
| PWM control | `CTL = 0x0000_a1e1` | MSEN2, USEF2, PWEN2, MSEN1, CLRF1, USEF1, PWEN1 |
| PWM DMA control | `DMAC = 0x8000_0e0e` | enable, panic 14, DREQ 14 |
| DMA transfer info | `TI = 0x0405_2149` | 32-bit, source increment, DREQ 5, INTEN |
| Period | 1024 frames | 8192 bytes of duty words (two words per frame) |
| DMA ring | 4 control blocks | one period per block, last points at the first |

The complete `CTL`/`DMAC`/`TI` derivation and donor citations are in the B41 report; the values
above are the B46 contract. `RPTL=0` and `SBIT=0` make an underrun play silence rather than stale
data.

### 1.1 Clock

The driver must stop and reprogram the PWM clock; the firmware state is UNVERIFIED.

```text
ctl = read32(CM + 0xa0)
write32(CM + 0xa0, 0x5a000000 | (ctl & ~0x10))   // password, clear ENAB
poll until (read32(CM + 0xa0) & 0x80) == 0       // BUSY=0, bounded
write32(CM + 0xa4, 0x5a000000 | (5 << 12))       // DIVI=5
write32(CM + 0xa0, 0x5a000016)                   // PLLD, ENAB, MASH=0
poll until (read32(CM + 0xa0) & 0x80) != 0       // BUSY=1, bounded
```

`CM_CTL_START = 0x5a00_0016` and `CM_DIV_START = 0x5a00_5000` are the constants in
`raven_pwm::clock`. The clock rate granted by `platd` must be 100 MHz; the driver refuses any
other rate instead of silently retuning.

### 1.2 GPIO

`GPFSEL4` bits 0..2 and 3..5 select GPIO40/41; ALT0 is `4`. `GPPUPPDN2` fields for pins 40/41 are
bits 16..17 and 18..19; `0` is pull-off on BCM2711. The driver reads both registers, changes only
those fields, and writes them back.

### 1.3 DMA

Each 32-byte control block is:

```text
TI        source       destination   length    stride    next
0x04052149 duty_bus   0x7e20c818    8192      0         next_cb_bus
```

Four blocks cover four 1024-frame periods. The duty buffer is 4 × 8192 = 32 KiB. The control
blocks live in the same S21d `DmaBuffer` window as the duty data; the bus address must be inside
the first 1 GiB DMA window for channels 0..10. The platform allocates the channel; channel 5 is
preferred but the driver never hardcodes it.

## 2. Crates

### 2.1 `crates/raven-pwm`

Pure, `no_std`, host-testable model:

| Module | Contents |
|---|---|
| `registers` | PWM/CPRMAN/GPIO/DMA offsets, control words, board bases |
| `clock` | PLLD/5 plan, range table, start/stop values, BUSY predicate |
| `gpio` | GPIO40/41 ALT0 and pull-off arithmetic with field preservation |
| `duty` | signed S16LE to `0..=RANGE` duty conversion, channel order, silence |
| `dma` | 32-byte control block encode/decode, ring geometry, bus addresses |
| `dtb_ut` | parses the `tests/fixtures/pi4-audio` fixture and checks the board bases |

`duty::convert_period` maps `-32768` to `0`, `0` to `RANGE/2` and positive samples above the
midpoint, saturating at the range. `SampleOrder::RightLeft` is the default for the jack: the first
FIFO word is the right channel. The order is UNVERIFIED on hardware; select `LeftRight` if a
left-only test plays on the right.

### 2.2 `userland/drivers/pwm-audio`

`Seam` is the board boundary: four register windows (`Pwm`, `Clock`, `Gpio`, `Dma`), a DMA
window, the GPIO/clock leases and a bounded delay. `PwmAudio` owns the bring-up sequence and the
PCM sink:

```text
PwmAudio::new(seam, config)   validate format, ring layout and DMA geometry
prepare()                     claim pins, check 100 MHz, clock, pins, PWM regs,
                              prefill silence, write four control blocks
start()                       write the first CB, CTL_AUDIO, DMA ACTIVE, ENABLE bit
service(pcm_region)           consume one period, convert to duty, write the next slot
stop()                        clear PWM CTL/DMAC, reset the DMA channel, clear ENABLE
```

`service` reads the B40 `RingHeader`, verifies the layout, and honours the stream state:
`Stopped` produces no write; `Running`/`Draining` consume a period; fewer than one period is an
underrun, which fills `RANGE/2` silence and increments the header underrun counter. The driver
advances its own duty slot each period and wraps after four. The B40 header remains owned by
`audiod`; the driver only writes the read index, underruns and position through `RingConsumer`.

The current `main.rs` is a skeleton: the real `devd`/`platd`/S21d mapping and the audiod profile
wiring are not in this ticket. Running it on a board only prints that fact.

## 3. PCM ring contract

| Field | Value |
|---|---|
| Format | 48 000 Hz (or 44 100 Hz), 2 channels, S16LE |
| Period | 1024 frames |
| Periods | 4 |
| Capacity | 4096 frames |
| Region length | `align_up(80, 64) + 4096 * 4` = 16 512 bytes |
| Ring data offset | 128 |
| Producer | audiod (`write_index`) |
| Consumer | pwm-audio (`read_index`) |
| Underrun counter | header `underruns`, incremented by the driver |

The region length must be exact; `RingLayout::new` rejects padding. The driver rejects a header
whose layout differs from its configuration, and a stream that is not stereo S16LE.

## 4. Host tests

```sh
cargo test -p raven-pwm
cargo test -p pwm-audio
```

The PWM tests cover the control words, clock plan and ppm error, GPIO field preservation, duty
conversion and saturation, control-block encoding, ring wrap and geometry rejection, and the DTB
fixture. The driver tests use a fake seam with four register windows, a 64 KiB DMA window, a
CPRMAN BUSY model and the real B40 ring; they cover prepare/start/stop, register writes, the CB
ring, PCM conversion, right/left order, underrun silence and counter, slot wrap, stopped streams,
bad formats, bad clock, refused pins and corrupt rings.

`make check-fast` is the integration gate. No test in this ticket touches hardware.

## 5. Board bring-up (UNVERIFIED)

Everything in this section is a procedure, not a result. Capture a log for each step.

### 5.1 Firmware configuration

`config.txt`:

```text
dtparam=audio=off
dtoverlay=pwm1,pins_40_41,pull_off
```

`audio=off` keeps the firmware audio service away from PWM1; the `pwm1` overlay enables the block
and muxes GPIO40/41. Do not combine `audio=on` with this driver. The exact overlay effect is
UNVERIFIED on the owner's firmware revision.

### 5.2 Pre-flight reads

Before the RavenBSD driver runs, read and record:

```text
0xfe200010  GPFSEL4        expect GPIO40/41 fields = 4 (ALT0)
0xfe2000ec  GPPUPPDN2      expect fields 16..17, 18..19 = 0 (pull-off)
0xfe1010a0  CM_PWMCTL      source, ENAB, BUSY before reprogramming
0xfe1010a4  CM_PWMDIV      DIVI/DIVF before reprogramming
0xfe20c800  CTL, STA, DMAC, RNG1, RNG2
0xfe007ff0  DMA ENABLE     live channel mask
0xfe007000 + 0x100*ch      CS, CONBLK_AD, DEBUG for the granted channel
```

Also record the live device tree (`pwm@7e20c800` status, `audio_pins`, both DMA masks) and the
mailbox `GET_CLOCKRATE` result for clock id `0x0a`. PLLD may differ under overclock; the driver
requires the granted rate to be exactly 100 MHz for the range table.

### 5.3 Driver sequence

1. `platd` grants GPIO40/41, the PWM clock and one DMA channel; S21d supplies a
   non-cacheable `DmaBuffer` with its bus address and length.
2. `PwmAudio::new` validates the B40 ring and the DMA geometry.
3. `prepare` reprograms the clock, pins, PWM and DMA ring, prefilling all four periods with
   `RANGE/2` silence.
4. `start` enables the FIFO-fed channels and the DMA channel.
5. audiod fills the ring, sets the header state to `Running` and calls `service` once per period.
6. `stop` silences the PWM, resets the DMA channel and clears its global enable bit.

The real entry point and the profile that launches it are not part of this ticket. The first
board run must therefore be treated as a smoke test of the integration, not of this library.

### 5.4 Audio checks

1. Play a 1 kHz sine at about -6 dBFS for 5 s at 48 kHz, then repeat at 44.1 kHz.
2. Record the jack on a PC line input at 48 kHz/24-bit. Check the fundamental (expected +160 ppm
   at 48 kHz is about 0.16 Hz, below the FFT resolution), channel separation and the absence of
   periodic clicks.
3. Repeat with left-only and right-only tones to settle the channel order. Record whether
   `RightLeft` or `LeftRight` is correct.
4. Compare with `dtparam=audio=on` as a reference, then with `audio=off` plus this driver, to
   prove the firmware is no longer driving the jack.
5. Record DC offset, noise/hum and harmonic levels; the jack is AC-coupled.

### 5.5 Evidence to attach

* The pre-flight register dump and live DTB.
* The `dmesg`/driver log showing the clock, pins, channel and range.
* The line-in recording or a spectrogram, with capture device and gain.
* The final `config.txt`.
* Any deviation from the values in section 1.

Until those exist, the board status is **UNVERIFIED**.

## 6. Troubleshooting

| Symptom | Check |
|---|---|
| Silence | PWM1 vs PWM0 base, `CTL` PWEN bits, `DMAC` enable, clock BUSY/ENAB, DMA ENABLE bit |
| One channel only | GPIO41 vs GPIO40 ALT0, `RNG1`/`RNG2`, CB length and slot parity |
| Clicks every period | underrun count, service cadence, `RPTL=0`, silence prefill |
| Wrong pitch | actual PLLD rate, `CM_PWMDIV`, `RNG1/2`, whether the range uses two channels |
| DMA stalls | channel ownership, bus address inside the first 1 GiB window, `TI` DREQ 5, CB `next` |
| Left/right swapped | `SampleOrder`; the hardware order is UNVERIFIED |

## 7. References

* `tickets/bob/reports/B41.md` - Pi 4 PWM audio spike: register sequence, donor citations, board
  evidence list.
* `tickets/bob/reports/B40.md` - `audio:` protocol and the shared PCM ring.
* `crates/raven-proto-audio/src/ring.rs` - ring header, layout and consumer.
* `tests/fixtures/pi4-audio/README.md` - fixture sources and decoded values.
* NetBSD `bcm2835_pwm.h`, `bcm2835_dmac.h`, `bcm2835_cm.h`, `bcm2835_gpioreg.h` - register facts.
* `bcm2711.dtsi` - PWM1, GPIO and CPRMAN node shapes.
* Public Linux `bcm2835-analog-audio` driver - `CTL=0xa1e1` / `DMAC=0x80000e0e` cross-check
  (EXTERNAL; constants checked, not copied).
