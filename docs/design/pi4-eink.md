<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 7.5-inch E-Ink core and integration plan

Status: **host core implemented and tested (OBSERVED); Pi 4 panel hardware UNVERIFIED.** E1 adds
the first-party Rust Canvas and UC8179 command encoder. It does not open SPI, touch a GPIO or
claim a hardware pass. The Pi 4 transport, service and measured refresh evidence are E2/E3 work.

## 1. Panel decision

| Item | Choice | Evidence |
|---|---|---|
| Panel class | Waveshare 7.5-inch V2-class, UC8179 controller, 800x480, 1-bit black/white | Vendor product/datasheet (OBSERVED); exact SKU/revision must be recorded when the panel arrives |
| Interface | 4-wire SPI: SCLK, MOSI, CS, DC; plus RST and BUSY | Datasheet (OBSERVED); pin plan below |
| Refresh | Full refresh only in E1; no partial-update LUT | E1 scope; partial refresh is a later optimization |
| Alternative rejected for 0.1.0 | IT8951 1872x1404 panels (higher resolution, more power/price, different protocol) | PROPOSED; revisit after 0.1.0 |

The command tables in `ravenbsd-rpi/crates/eink` (`ravenbsd-eink`) are the UC8179 full-refresh constants for this panel
class. They are hardware register values from the vendor documentation, not copied software. The
encoder is proven against a recording mock, not against a panel (UNVERIFIED).

## 2. Pi 4 pinout

The panel HAT/breakout uses the Pi 4 40-pin header. Verify the silkscreen and panel revision before
powering the panel; the HAT mapping varies between Waveshare revisions.

| Signal | Pi 4 header pin | BCM GPIO | Notes |
|---|---|---|---|
| 3V3 | 1 or 17 | — | Panel logic supply; check the panel board's regulator/level shifters |
| GND | 6, 9, 14, 20, 25, 30, 34, 39 | — | Common ground with the Pi |
| SCLK / CLK | 23 | GPIO11 | SPI0 SCLK |
| MOSI / DIN | 19 | GPIO10 | SPI0 MOSI |
| CS / CE0 | 24 | GPIO8 | SPI0 CE0, active low |
| DC | 22 | GPIO25 | Low = command, high = data |
| RST | 11 | GPIO17 | Active low; hold low at least 10 ms |
| BUSY | 18 | GPIO24 | High = panel busy; poll with a bounded delay |
| PWR (optional) | — | — | Panel power switch; E1 leaves it unconnected |

SPI mode 0, 8 bits, MSB first, CS active low. Start at **4 MHz** and raise only after a measured
signal-integrity check; the panel timing is dominated by refresh, not the SPI clock. DC/RST/BUSY
are plain GPIOs and must not be driven by the SPI peripheral.

## 3. Power and refresh timing

* 3.3 V logic and panel supply; budget peak refresh current from the panel datasheet and add local
  decoupling. Do not power the panel from a GPIO.
* Reset: RST low >= 10 ms, then high; wait for BUSY release.
* Power-on settling: 100 ms (encoder constant), then BUSY wait.
* Full refresh: the panel's full-refresh waveform takes on the order of two seconds; the encoder
  does not sleep for a guessed duration, it waits for BUSY with a bounded 10 s diagnostic guard
  (`MAX_BUSY_POLLS` × 10 ms). A real refresh time is measured in E2.
* Deep sleep: power off, then the UC8179 deep-sleep command with the `0xA5` check byte. Wake
  requires a reset.
* Partial refresh and fast waveforms are out of E1; they need a measured ghosting/refresh budget.

## 4. Pi 4 integration: platd or a dedicated driver

`platd` currently owns GPIO, mailbox, RNG and DMA; it has no SPI driver. E1 deliberately does not
edit `platd` or the kernel.

**Recommendation: a dedicated userland SPI/E-Ink service (`einkd`) with a small `spi-eink`
transport, using `platd` only for DC/RST/BUSY GPIO if a GPIO client is available.** Reasons:

* A full refresh blocks for seconds; a BUSY wait inside `platd` would stall every platform client.
* The panel is a low-frequency, failure-tolerant device; a separate service can retry, keep the
  last good frame and be restarted without resetting the platform service.
* The transport stays first-party Rust and testable through the `Transport`/`Delay` seam.
* The alternative (extend `platd` with SPI0) is viable for a future fast path, but couples a slow
  panel to the platform daemon and is not needed for 0.1.0.

The SPI0 MMIO/GPIO mapping and the exact GPIO client are E2 decisions; no code in E1 assumes a
kernel or `platd` change.

## 5. Service pipeline and scheduling

```
boot -> login -> einkd starts (after login/network readiness)
     -> fetch content (E3; HTTPS/TLS decision)
     -> render into Canvas (rotation, text/glyphs, lines/rects)
     -> pack via PanelProfile -> Uc8179 data + full refresh
     -> keep last-good frame; schedule next refresh
```

* Start `einkd` **after login**, not in the getty critical path. The boot/login gate must remain
  unaffected; a 2 s panel refresh is not part of the <1 s boot measurement.
* NTP/time: wait for the time service/NTP sync before content with a clock, but do not block the
  first frame on time if the last-good page is valid. A scheduled refresh uses the NTP-corrected
  wall clock; retry/backoff on fetch or panel failure.
* Refresh policy: full refresh only; no partial refresh in 0.1.0. A failed fetch keeps the last
  good frame visible and does not cycle the panel needlessly.
* Power: deep sleep between long intervals; reset/wake before the next frame.

## 6. <1 s login impact and evidence plan

The panel is not opened during boot or login. `einkd` starts after login and the full refresh runs
in its own process. The S84 boot-time gate therefore measures the same kernel/getty path with and
without `einkd`; the expected delta is zero before the first scheduled refresh. E2/E3 must record:

* host: `cargo nextest run` in `ravenbsd-rpi`, `make check-fast` (E1);
* Pi 4: SPI loopback, DC/RST/BUSY timing, BUSY high duration and full-refresh wall time;
* Pi 4: last-good-frame after a failed fetch and a clean einkd restart;
* boot/login: the S84/S89 boot script with `einkd` disabled and enabled, reporting the delta
  separately; no panel refresh on the login path.

Host evidence is OBSERVED. Pi 4/panel evidence is UNVERIFIED until the hardware run is recorded.

## 7. BOM

| Item | Quantity | Note |
|---|---|---|
| Waveshare 7.5-inch V2-class UC8179 panel | 1 | 800x480, 1-bit, SPI; record SKU/revision |
| Pi 4 40-pin HAT/breakout or jumper set | 1 | Verify the silkscreen pin mapping |
| 3.3 V / GND leads | 1 set | Panel logic supply; local decoupling on the HAT |
| microSD card | 1 | Release image |
| Pi 4 power supply | 1 | Panel refresh current is additional |
| Optional panel power switch | 1 | E1 leaves PWR unconnected |

## 8. DR-52 donor reconciliation

Only behavioural ideas were consulted. No NetBSD/FreeBSD/Linux/Illumos source code is copied; the
crate is first-party BSD-3-Clause Rust. The UC8179 command/LUT tables are hardware datasheet
constants, not donor code.

| Donor / area | Idea | Applied in E1 | Remaining work |
|---|---|---|---|
| NetBSD/FreeBSD SPI and GPIO frameworks | Separate controller/device layers; busy polling with bounded waits | `Transport`/`Delay` seam; bounded BUSY poll | Pi 4 SPI0/GPIO transport and wiring (E2) |
| Linux fbtft / E-Ink panel drivers | Canvas-to-panel command/data split; rotation and packing at the edge | 1-bit Canvas, rotation, profile packing, UC8179 encoder | Hardware timing, waveform validation, partial refresh |
| Illumos device-driver layering | Explicit device lifecycle and power states | Reset/power-on/refresh/deep-sleep encoder | Service integration and power policy (E3) |
| Pi 4 BCM2711 documentation | SPI0 CE0/CLK/MOSI mapping | Documented pin plan; no code copied | E2 MMIO/GPIO implementation |

## 9. Open inputs for E2/E3

* Exact panel SKU, revision and HAT pin mapping; measure 3.3 V current during refresh.
* SPI clock/signal integrity and the real BUSY/refresh timing.
* `platd` GPIO client availability versus a dedicated GPIO mapping in `einkd`.
* HTTPS/TLS and fetch ownership (E3 follows the panel/TLS decision).
* Whether the panel is fitted to a HAT or wired directly; enclosure/power routing is mechanical.
