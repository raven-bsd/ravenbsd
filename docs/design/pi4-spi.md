<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 SPI0 master core and E-Ink transport plan

Status: **host core implemented and tested (OBSERVED); Pi 4 SPI/panel hardware UNVERIFIED.**
`ravenbsd-rpi/crates/pi-spi` (`ravenbsd-pi-spi`) is the first-party BCM2711 SPI0/PL022-class register and transfer core. It
has no MMIO implementation, no GPIO access and no hardware claim. E2 supplies the mapped window
and the Pi 4 evidence.

## 1. Controller

| Item | Value | Evidence |
|---|---|---|
| Controller | BCM2711 SPI0, PL022-class FIFO master | BCM2711/PL022 documentation (OBSERVED) |
| Physical base | `0xFE20_4000` | BCM2711 address map (OBSERVED) |
| Window used | `0x00..0x18`: CS, FIFO, CLK, DLEN, LTOH, DC | Register model in `raven-spi` |
| Data width | 8-bit FIFO transfers | E1 panel uses 8-bit command/data |
| Chip selects | CE0 (GPIO8), CE1 (GPIO7) | ALT0 pinmux |
| Core clock | 250 MHz nominal input to the divider; measure the board value | E2 input |

Register model:

| Offset | Register | Use |
|---|---|---|
| `0x00` | `CS` | CE field, CPOL/CPHA, FIFO clear, TA, RXD/TXD flags |
| `0x04` | `FIFO` | 8-bit TX/RX data |
| `0x08` | `CLK` | 16-bit clock divider |
| `0x0c` | `DLEN` | transfer length |
| `0x10` | `LTOH` | LoSSI mode (0 for SPI) |
| `0x14` | `DC` | DMA DREQ control (0 for polling) |

The core clears both FIFOs, sets `DLEN`, asserts the selected CE with `TA`, waits for `TXD`
before each byte, waits for `RXD` before reading, and deasserts CE at the end. Every wait is
bounded by `timeout_polls`; a stuck FIFO returns `Error::Timeout`. The divider is
`ceil(core_hz / target_hz)`, clamped to a minimum of 2 and a 16-bit maximum.

## 2. Pinmux and board wiring

| Signal | GPIO | Pi 4 header | Function |
|---|---|---|---|
| SPI0 CE1 | GPIO7 | pin 26 | ALT0 |
| SPI0 CE0 | GPIO8 | pin 24 | ALT0 |
| SPI0 MISO | GPIO9 | pin 21 | ALT0 (unused by the E-Ink panel) |
| SPI0 MOSI | GPIO10 | pin 19 | ALT0 |
| SPI0 SCLK | GPIO11 | pin 23 | ALT0 |
| 3V3 | — | pin 1/17 | Panel logic supply |
| GND | — | pin 6/9/14/20/25/30/34/39 | Common ground |

The panel's DC/RST/BUSY are ordinary GPIOs (E1 pin plan: DC GPIO25, RST GPIO17, BUSY GPIO24),
not SPI signals. SPI mode 0, 8 bits, MSB first, CE active low. Start at 4 MHz; measure signal
integrity before raising the clock. The E-Ink refresh, not the SPI clock, dominates wall time.

## 3. Polling versus DMA/IRQ

E1b implements the polling path: the panel frame is 48,000 bytes per refresh, and at 4 MHz the
SPI part is roughly 0.1 s while the panel refresh is on the order of seconds. Polling keeps the
first integration small and deterministic; the bounded poll makes a stuck controller a timeout
rather than a hang.

DMA/IRQ is a later optimization:

* `DLEN`/`DC` and the FIFO are already modelled, so a DMA backend can reuse the register map.
* DMA needs a bus-addressable frame buffer and completion IRQ; it is not needed to prove the
  panel path and would add boot-memory/ownership work.
* A DMA driver must still bound the completion wait; it must not reintroduce an unbounded spin.

## 4. platd or a dedicated SPI driver

`platd` owns GPIO/mailbox/RNG/DMA and has no SPI driver. E1b does not edit it. Recommendation:
a dedicated userland `einkd`/`spi-eink` service owns SPI0 and the panel, using `platd` only for
DC/RST/BUSY GPIO if a GPIO client is available.

* A full refresh blocks for seconds; it must not stall the platform daemon.
* The E-Ink service can retry, retain the last-good frame and restart independently.
* The register/transfer core is `#![forbid(unsafe_code)]`; only the E2 `RegisterBlock` wrapper
  needs the mapped volatile window (preferably via `raven-driver::MmioRegion`).
* Extending `platd` with SPI0 remains an option for a future shared bus, but it is not required
  for 0.1.0 and couples a slow panel to every platform client.

## 5. E-Ink timing and service path

```
login -> einkd starts after login
      -> fetch/render/pack (E3)
      -> spi.write(CE0, command/data chunks; DC low/high)
      -> UC8179 data start + full refresh
      -> BUSY wait (panel E1: bounded 10 s)
      -> keep last-good frame; deep sleep between long intervals
```

The SPI core is only the byte transport. The E1 encoder owns reset/power/LUT/full-refresh
commands, DC state and the bounded BUSY poll. No SPI transfer runs on the boot/login path; the
<1 s login gate is measured with `einkd` disabled and enabled separately. A failed SPI transfer or
a panel timeout leaves the last-good frame visible and is retried with backoff, not by hiding the
error.

## 6. Pi 4 evidence plan

Host (OBSERVED):

* `cargo nextest run -p raven-spi` covers constants/divisor, configure, byte order, CE0/CE1,
  full-duplex/write-only, FIFO full/empty timeouts, length mismatch and back-to-back CS toggles.
* `make check-fast` is the ticket gate; no hardware is opened.

Pi 4 (UNVERIFIED, E2):

* Confirm the SPI0 mapping, core clock and ALT0 pinmux; measure the actual `CLK` rate.
* Loopback MOSI→MISO on the header; check byte order and CS timing with a scope/logic analyzer.
* Measure one 48,000-byte transfer and one UC8179 full refresh; verify DC/RST/BUSY wiring.
* Confirm the panel image on the real 7.5-inch panel; record the exact SKU/revision.
* Confirm the boot/login delta with `einkd` disabled/enabled.

## 7. BOM

| Item | Quantity | Note |
|---|---|---|
| Raspberry Pi 4 | 1 | SPI0 master |
| Waveshare 7.5-inch V2-class UC8179 panel | 1 | 800x480, 1-bit |
| 40-pin header/jumper set | 1 | SPI0 + DC/RST/BUSY; verify silkscreen |
| 3.3 V/GND leads and decoupling | 1 set | Do not power the panel from a GPIO |
| microSD card and Pi 4 PSU | 1 each | Release image; panel current is additional |
| Logic analyzer/scope (bring-up) | 1 | CS/CLK/MOSI/DC timing and loopback |

## 8. DR-57 / DR-52 reconciliation

No donor source code is copied. The register map and command values are hardware documentation;
the core is first-party BSD-3-Clause Rust. DR-52 is the general donor reconciliation; DR-57 is
the SPI/PL022-specific disposition.

| Donor / area | Idea | Applied | Remaining |
|---|---|---|---|
| NetBSD/FreeBSD SPI and PL022 frameworks | FIFO flag polling, CS ownership, divisor selection | `RegisterBlock`, bounded `TXD`/`RXD` waits, divisor helper | Pi 4 mapped-window wrapper and GPIO ownership (E2) |
| Linux `spi-bcm2835` (design reference) | BCM SPI0 CS/FIFO/CLK/DLEN model; polling first, DMA later | Register/bit model, 8-bit FIFO path, polling core | DMA/IRQ backend if a later ticket needs it |
| PL022 datasheet | Master FIFO transfers and status flags | 8-bit full-duplex/write-only core | Hardware FIFO depth/timing measurement |
| Illumos device layering | Explicit device lifecycle and error paths | `Error` variants and bounded failures | Service lifecycle in `einkd` (E3) |
| `platd`/E1 design | Dedicated E-Ink service, GPIO for DC/RST/BUSY | Design decision above | E2 transport, E3 fetch/render |

## 9. Open hardware inputs

* Exact Pi 4 core clock and measured SPI rate; `CORE_CLOCK_HZ` is only a nominal example.
* Whether `platd` exposes the needed GPIO client or E2 maps the GPIO window directly.
* Panel SKU/revision and the HAT pin mapping; 3.3 V current during refresh.
* DMA/IRQ need and frame-buffer ownership after the first panel bring-up.
