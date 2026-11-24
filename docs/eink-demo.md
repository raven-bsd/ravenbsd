<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 E-Ink demo (E4)

Status: **host adapters, the SPI0 scheme driver and image staging are OBSERVED; the real panel
run is UNVERIFIED because no Pi 4 is attached to this session.** The clean 0.1.0 path is wired:
`spi0d` owns SPI0 and serves `spi:`, `platd` drives DC/RST/BUSY, and `einkd` stays an `svcd`
service. No panel, network or NTP board result is claimed; UI1/E5 take the board transcript and
photo.

## 1. Hardware

* Waveshare 7.5-inch E-Paper HAT, 800x480, black/white, UC8179-class, full refresh.
* Raspberry Pi 4, 3.3 V logic, SPI0:
  SCLK GPIO11, MOSI GPIO10, CE0 GPIO8, DC GPIO25, RST GPIO17, BUSY GPIO24.
* The board data is `boards/pi4/panel-waveshare-7in5-v2.toml` in the BSP repo and the
  `ravenbsd-eink` UC8179 encoder.
* E-Ink uses the [`Canvas`] model. The HDMI path uses display-buffer terminology; E-Ink never
  uses a framebuffer type.

## 2. Software path

```
raven-http/raven-http-net (plain HTTP GET over net:/resolv:)
        |
        v
apps/einkd: Content -> render text into 800x480 Canvas
        |
        v
einkd HardwareTransport -> plat: GPIO (DC/RST/BUSY) + spi: transfers
        |
        v
spi0d (devd driver, SPI0 MMIO) -> UC8179 command/data -> panel full refresh
        |
        v
SNTP offset -> SntpClock -> next refresh interval
```

`spi0d` is a small devd-launched platform driver: it owns the SPI0 MMIO window, sets CE0 through
`ravenbsd-pi-spi`, and serves the `spi:` character endpoint. `einkd` opens `spi:0`, so `svcd`
still owns the service namespace and the network/resolver/plat dependencies. The panel BUSY wait
is in `einkd`, never `platd`.

* `HttpContent` is `raven-http`'s bounded GET client over `raven-http-net`'s `net:`/`resolv:`
  transport. HTTPS is rejected, never downgraded (DR-45).
* `HardwareTransport` is host-tested with recording SPI/GPIO mocks; the panel init, LUT load,
  full frame and BUSY wait are the existing UC8179 sequence.
* `SntpClock` keeps the SNTP offset in the service because 0.1.0 has no `CLOCK_SET` syscall. The
  schedule is NTP-corrected; the system clock is not changed.
* Panel init happens on the first successful fetch, after login and network. No panel access
  happens on the boot/getty path. Fetch and panel failures keep the last-good Canvas and use the
  service exponential backoff.

## 3. Configuration

`/etc/einkd.conf` (staged from `profiles/pi4-eink.d/einkd.conf`):

```
url=http://192.0.2.10/eink
interval_secs=300
retry_secs=5
max_backoff_secs=300
ntp_server=162.159.200.1
ntp_port=123
```

The TEST-NET URL is a placeholder; the owner sets the approved plain-HTTP source before the
board run. `interval_secs` is the full-refresh period. The service entry is
`/etc/services.d/einkd.toml`; it needs `getty`, `scheme:net`, `scheme:resolv` and `scheme:plat`,
and stays `enable = false` until the launch path below exists.

## 4. Launch/handle path

The clean 0.1.0 path is wired:

1. **SPI0:** `devd` matches `brcm,bcm2835-spi` and launches `/sbin/spi0d` with the SPI0 reg window
   (`bar=3`, `bar_size=24`). `spi0d` maps it, configures 4 MHz and registers `spi:`. The profile
   also stages a disabled `spi0d` svcd entry whose only role is to publish the `spi` prefix to
   `einkd`.
2. **GPIO:** `plat:` now has `GPIO_SET_LEVEL` (`raven-plat` + `platd`); `einkd` claims
   `plat:gpio/17,24,25`, sets DC/RST as outputs and BUSY as input, and drives the levels. The
   direct `--gpio=H:L` path remains for diagnostics.
3. **Service:** `einkd` stays an `svcd` service (`needs getty, scheme:net, scheme:resolv,
   scheme:plat, scheme:spi`), so `net:`/`resolv:` and the panel path are in one process. The
   profile enables it; the first successful fetch initialises the panel, so boot/getty never
   touches it.
4. **DTB:** the live Pi 4 firmware DTB must be the `/etc/pi4.dtb` source for the SPI0 node to
   match; the synthetic host fixture does not include SPI0. That staging is a UI1/E5 board item.

## 5. Failure modes

| Failure | Behaviour |
|---|---|
| DNS/connect/HTTP error | `FetchFailed`, last-good Canvas stays, exponential backoff |
| non-2xx response | fetch failure, no render |
| SNTP timeout | log `ntp unavailable`, continue on the wall clock |
| BUSY timeout | `PanelFailed`, last-good Canvas stays, backoff |
| panel/SPI error | same panel backoff, no boot/getty impact |
| service disabled | no panel access at all; UART/login unchanged |

## 6. Host and image evidence

* `cargo nextest run` in `ravenbsd-rpi`: hardware transport, HTTP content, SNTP clock, config
  parser, GPIO math and the `spi:` handler tests (69 total).
* `cargo clippy --all-targets -- -D warnings` in the BSP: clean.
* `cargo check -p einkd -p spi0d --target aarch64-unknown-none-softfloat`: target build.
* `sh scripts/build.sh`: stages `build/aarch64/einkd` and `build/aarch64/spi0d`.
* `cargo xtask image --arch aarch64 --platform pi4 --profile pi4-eink` (product): builds the
  image; the initfs contains `/sbin/einkd`, `/sbin/spi0d`, `/etc/einkd.conf`, both manifests,
  `netd`, `resolvd`, `platd` and the driver table.
* `make check-fast` in the product at the E4 branch.

SD2 is merged in the current base (`a32745a5`); the RW-root boot chain still needs its SD3 board
transcript. No board transcript exists in this session.

## 7. Board steps / E5 photo

1. Build the BSP binary: `cd ravenbsd-rpi && sh scripts/build.sh`.
2. Build and flash the image:
   `RAVEN_RPI_FIRMWARE=<rpi-firmware>/boot cargo xtask image --arch aarch64 --platform pi4 --profile pi4-eink`.
3. Connect UART at 115200 and the panel HAT. Boot; confirm the normal kernel/login transcript.
4. The profile enables `einkd`; `devd` launches `spi0d` and `platd`, then `einkd` logs
   `einkd: ntp synced ...`, the fetch result, `einkd: refreshed next_at=...` and the panel shows
   the page. Capture the serial transcript.
5. E5 takes the photo of the panel and records the panel SKU, cable, PSU, image SHA and the serial
   transcript. Do not use a screenshot or a fake frame.

## 8. DR-57 / DR-52 reconciliation

Donor revisions: NetBSD `36eddebcc36cb0775aa2962005b9c05419a6ff01`, FreeBSD
`133344405f532bd54ff3629a77d4c4e062f01fdb`.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| SPI0/PL022 transport | No BCM2711 SPI | FreeBSD `bcm2835_spi.c` (BSD-2); NetBSD `bcm2835_spi.c` (BSD-2) | `spi-bcm2835` outcome only | Existing first-party `ravenbsd-pi-spi` core; adapter owns only MMIO mapping |
| GPIO function/level | No BCM GPIO | FreeBSD `bcm2835_gpio.c` (BSD-2); NetBSD `bcm2835_gpio.c` (BSD-2) | `pinctrl-bcm2835` outcome | First-party GPFSEL/GPSET/GPCLR/GPLEV math; `plat:` claim/function/level (`GPIO_SET_LEVEL`) |
| UC8179 panel | No E-Ink | Vendor UC8179 datasheet constants (E1) | no copy | Existing `ravenbsd-eink` encoder; full refresh only |
| HTTP fetch | Illumos no HTTP client in scope | E3 first-party `raven-http`/`raven-http-net` | outcome only | Reuse E3; no TLS downgrade |
| Time | Illumos `ntpd` outcome | E3 first-party `raven-sntp` | outcome only | SNTP offset in the service; no system clock set |
| Service order | SMF dependency graph | CON1/E1e manifest model | systemd order outcome | einkd after getty/net/resolv/plat; no boot/getty panel access |

No C, no copied CDDL/GPL, and no new donor-derived table.

## 9. Open inputs

* Owner-approved plain-HTTP content URL.
* Live Pi 4 firmware DTB staged as `/etc/pi4.dtb` so devd can match the SPI0 node (the synthetic
  host fixture is not enough for the board).
* SD3 first real-card boot log and E5 panel photo/serial transcript.
