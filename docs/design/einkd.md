<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# einkd service core and Pi 4 integration plan

Status: **host core implemented and tested (OBSERVED); Pi 4 SPI/GPIO/panel and network fetch are
UNVERIFIED.** `userland/servers/einkd` owns the fetch→render→refresh state machine. It is
`no_std`, has no threads, sleeps, HTTP, SPI or GPIO access; every effect is an injected seam.

## 1. Configuration

```toml
# Proposed einkd configuration; the E3 loader supplies the real values.
profile = "waveshare-7.5-v2"   # 800x480 UC8179, full refresh only
rotation = "none"              # none | quarter | half | three-quarter
interval_secs = 300            # scheduled refresh interval
retry_secs = 5                 # first fetch/panel failure backoff
max_backoff_secs = 300         # exponential ceiling
source = "eink"                # E3 content source name
```

The core `Config` validates: profile is the supported UC8179 class, interval/retry are non-zero,
the backoff ceiling is at least the first retry, and the source is not empty. A source/URL is an
E3 input; the service never opens a network socket itself.

## 2. State machine

```
                 ready=false
       +--------------------------+
       |                          v
   [Waiting] --ready=true--> [Fetching] --ok--> [Rendering] --> [Refreshing]
                                  |                               |
                                  | fetch error                   | panel error
                                  v                               v
                              [Backoff] <-------------------------+
                                  |
                                  | retry timer
                                  v
                              [Fetching]

   any state --shutdown()--> [Shutdown]
```

* **Waiting** — login/network/NTP is not ready. No fetch and no panel access.
* **Fetching** — call `Content::fetch`; the page borrows the provider buffer.
* **Rendering** — clear the working framebuffer and draw the page (bitmap or text via `Font`).
* **Refreshing** — call `Panel::refresh`; the UC8179 adapter initialises once, packs the frame and
  requests a full refresh.
* **Idle** — a successful frame is displayed; wait until `next_refresh_at`.
* **Backoff** — fetch or panel failed; the last-good frame remains visible and the next attempt is
  scheduled with exponential backoff (5 s, 10 s, 20 s … up to `max_backoff_secs`).
* **Shutdown** — clean stop; no further fetch/panel access.

The caller drives `tick(ready)` from a post-login worker. The core never sleeps or spins; the
scheduler supplies `Clock::now_secs()` (NTP-corrected wall time).

## 3. Refresh policy and last-good

* **Full refresh only** in 0.1.0. Partial refresh/ghosting is a later measured optimization.
* `last_good` is the frame that the panel successfully accepted. A fetch failure, a render with
  unsupported glyphs, or a panel timeout never changes it.
* On a successful panel refresh, the working and last-good buffers swap; the next render clears
  the working buffer.
* A failed fetch/panel access is retried with backoff; the error is not hidden by a fake refresh.
* The first frame has no last-good predecessor; if the first fetch/panel fails, the panel keeps
  whatever it was showing (the service does not paint a synthetic "OK" page).
* The service starts after login/network readiness and never runs on the getty/boot path.

## 4. Profile ordering and UC8179 path

1. Configure the SPI clock/CS and GPIO DC/RST/BUSY (E2 transport).
2. Reset the panel; power on; wait BUSY.
3. Load panel setting, booster, PLL, resolution, VCOM and full-refresh LUTs (E1 encoder).
4. Render the page into the 800x480 framebuffer with the configured rotation.
5. Pack to the panel row format (1=black in the framebuffer → 1=white in panel RAM).
6. `DATA_START`, send the 48,000-byte frame, `DISPLAY_REFRESH`, wait BUSY.
7. Keep the frame as last-good; deep sleep between long intervals, reset/wake next time.

The service `Uc8179Panel` adapter initialises the E1 encoder once and reuses it. It writes through
`raven_eink::Transport`/`Delay`; the E2 SPI/GPIO transport is a separate implementation.

## 5. E3 fetch integration

`Content::fetch(&mut self) -> Result<Page<'_>, Error>` is the only fetch seam:

* E3 implements HTTP/HTTPS (TLS decision) and returns either a 1-bit bitmap or `Page::Text`.
* `Page::Text` is rendered through `Font::glyph(u8)`; the glyph table and its licensing are an
  explicit E3 input, not a hidden asset in the service.
* A provider owns its page buffer; the service copies it into the framebuffer during `tick`, so
  the buffer can be reused after rendering.
* The `Config::source` name is passed to the E3 provider; it is not interpreted by the core.
* No network/HTTP/TLS claim is made by E1c.

## 6. platd / GPIO contract

`platd` remains the platform GPIO owner unless E2 adds an SPI bus. E1c does not edit it. The E2
transport services the panel pins:

| Signal | Direction | Contract |
|---|---|---|
| DC | einkd → panel | Low = command, high = data; changed before each SPI phase |
| RST | einkd → panel | Active low; reset pulse >= 10 ms, then BUSY wait |
| BUSY | panel → einkd | High = busy; polled with the E1 bounded wait, never on the login path |
| CE0 | SPI controller | Active low for the whole command/data phase |

If `platd` exposes a GPIO client, `einkd` uses it; otherwise E2 maps the GPIO window in the
dedicated service. The panel refresh runs in `einkd`, not `platd`, so a multi-second BUSY wait
cannot stall the platform daemon or getty.

## 7. Pi 4 evidence plan

Host (OBSERVED):

* `cargo nextest run -p einkd` covers readiness, successful refresh, fetch failure/last-good,
  panel failure/backoff, interval scheduling, text rendering, clean shutdown and the recorded
  UC8179 command/data sequence.
* `make check-fast` is the ticket gate.

Pi 4 (UNVERIFIED, E4):

* Wire SPI0 CE0/CLK/MOSI + DC/RST/BUSY; verify ALT0 pinmux and 3.3 V levels.
* Confirm the E2 SPI transport with a loopback/scope before attaching the panel.
* Display a known page, measure the full-refresh time and BUSY high duration.
* Fail the fetch and the panel deliberately: the last-good page must remain visible and the
  backoff must not spin the panel.
* Measure the boot/login delta with `einkd` disabled/enabled; the refresh is post-login only.
* Record the exact panel SKU/revision; no Pi 4 pass is claimed by E1c.

## 8. BOM (service-side)

The panel/SPI BOM is in `docs/design/pi4-eink.md` and `docs/design/pi4-spi.md`: Pi 4, Waveshare
7.5-inch V2-class UC8179 panel, 40-pin header/jumper set, 3.3 V/GND and decoupling, microSD/PSU,
and a logic analyzer for bring-up. `einkd` adds no hardware; E3/E4 own the fetch and physical
evidence.

## 9. DR-57 / DR-52 reconciliation

First-party BSD-3-Clause Rust; no driver/service code is copied. The panel command constants are
hardware documentation. DR-52 is the general donor reconciliation; DR-57 is the SPI/PL022 and
E-Ink service disposition.

| Donor / area | Idea | Applied | Remaining |
|---|---|---|---|
| NetBSD/FreeBSD service layering | Explicit start gate, retry/backoff, last-good state | `Service` state machine and `Action` results | E3 supervision/restart integration |
| Linux fbtft/panel services | Fetch→render→refresh separation, full-refresh policy | `Content`/`Panel` seams, full refresh only | Panel timing and image validation (E4) |
| Illumos/SMF design | Dependency-aware start after network/time | `tick(ready)` gate; NTP-corrected `Clock` | einkd service manifest/profile in E3 |
| `raven-eink` / `raven-spi` (first-party) | UC8179 encoder + BCM SPI transport | `Uc8179Panel` adapter and host mocks | E2 mapped SPI/GPIO transport |
| `platd` GPIO design | DC/RST/BUSY ownership, no slow work in platd | Documented contract; no platd edit | E2/E3 GPIO client decision |

## 10. Open inputs

* E3 HTTP/HTTPS/TLS source and the real `Font` glyph table/licensing.
* einkd service manifest/profile and its post-login supervision Graph (no getty dependency).
* E2 SPI/GPIO transport and E4 panel SKU/timing/current evidence.
* Whether the panel needs a board-specific reset/power sequence beyond the E1 UC8179 encoder.
