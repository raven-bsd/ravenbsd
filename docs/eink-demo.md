<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 E-Ink demo (E4)

Status: **host adapters and image staging OBSERVED; the real panel run is BLOCKED on the launch
path and is UNVERIFIED.** The Pi 4 board is not attached to this session, and the 0.1.0 product
does not yet provide an SPI0 handle to an `svcd` service or a `platd` GPIO level call. No panel,
network or NTP board result is claimed; E5 takes the photo after the blockers are closed.

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
Uc8179 -> HardwareTransport (DC/RST/BUSY + SPI0) -> panel full refresh
        |
        v
SNTP offset -> SntpClock -> next refresh interval
```

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

## 4. Launch/handle path (blocker)

The app accepts `--spi=H:L` and `--gpio=H:L` (mapped MMIO handles) and otherwise claims
`plat:gpio/17,24,25`. The current product has two gaps:

1. **SPI0 handle:** `svcd` services receive scheme prefixes and the initfs, not MMIO. `devd` can
   create an SPI0 window for a platform driver, but its child does not get the `net:`/`resolv:`
   scheme namespace, so the same process cannot fetch content. The app supports the devd style
   `--spi=H:L` once a launcher/kernel SPI resource driver supplies the handle.
2. **GPIO level:** `plat:` exposes `GPIO_SET_FUNCTION`, `GPIO_SET_PULL` and `GPIO_LEVEL`; there
   is no level-set call. DC must toggle command/data and RST must pulse high; the adapter reports
   the missing operation instead of leaving the panel in reset. The direct `--gpio=H:L` path
   uses the BCM2711 GPFSEL/GPSET/GPCLR/GPLEV registers and is host-tested, but no current launcher
   gives einkd that window.

Both need a product-side change (`crates/raven-plat`/`userland/servers/platd` level call and a
SPI0 resource/scheme grant) or a kernel SPI resource driver. They are outside E4's product
Owns paths and are recorded as the board blockers.

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

* `cargo nextest run` in `ravenbsd-rpi`: 59 tests, including the new hardware transport, HTTP
  content, SNTP clock and config parser tests.
* `cargo check -p einkd --target aarch64-unknown-none-softfloat`: target build.
* `sh scripts/build.sh`: stages `build/aarch64/einkd`.
* `cargo xtask image --arch aarch64 --platform pi4 --profile pi4-eink` (product): builds the
  image; the initfs contains `/sbin/einkd`, `/etc/einkd.conf`, the einkd manifest, `netd`,
  `resolvd` and `platd`.
* `make check-fast` in the product at the E4 branch.

No board transcript exists here. SD2's EMMC2/UFS read-write root commit is not in
`origin/master` at the E4 base (`6daa0fe0`), so the RW-root boot chain remains SD2/SD3 work.

## 7. Board steps / E5 photo

1. Build the BSP binary: `cd ravenbsd-rpi && sh scripts/build.sh`.
2. Build and flash the image:
   `RAVEN_RPI_FIRMWARE=<rpi-firmware>/boot cargo xtask image --arch aarch64 --platform pi4 --profile pi4-eink`.
3. Connect UART at 115200 and the panel HAT. Boot; confirm the normal kernel/login transcript.
4. After the SPI0/GPIO launch path is available, enable the einkd service (or start it with the
   handles) and capture the serial lines: `einkd: ntp synced ...`, `fetch ...`,
   `einkd: refreshed next_at=...`, and the panel photo.
5. E5 takes the photo of the panel and records the panel SKU, cable, PSU, image SHA and the serial
   transcript. Do not use a screenshot or a fake frame.

## 8. DR-57 / DR-52 reconciliation

Donor revisions: NetBSD `36eddebcc36cb0775aa2962005b9c05419a6ff01`, FreeBSD
`133344405f532bd54ff3629a77d4c4e062f01fdb`.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| SPI0/PL022 transport | No BCM2711 SPI | FreeBSD `bcm2835_spi.c` (BSD-2); NetBSD `bcm2835_spi.c` (BSD-2) | `spi-bcm2835` outcome only | Existing first-party `ravenbsd-pi-spi` core; adapter owns only MMIO mapping |
| GPIO function/level | No BCM GPIO | FreeBSD `bcm2835_gpio.c` (BSD-2); NetBSD `bcm2835_gpio.c` (BSD-2) | `pinctrl-bcm2835` outcome | First-party GPFSEL/GPSET/GPCLR/GPLEV math; `plat:` claim/function, direct path for level |
| UC8179 panel | No E-Ink | Vendor UC8179 datasheet constants (E1) | no copy | Existing `ravenbsd-eink` encoder; full refresh only |
| HTTP fetch | Illumos no HTTP client in scope | E3 first-party `raven-http`/`raven-http-net` | outcome only | Reuse E3; no TLS downgrade |
| Time | Illumos `ntpd` outcome | E3 first-party `raven-sntp` | outcome only | SNTP offset in the service; no system clock set |
| Service order | SMF dependency graph | CON1/E1e manifest model | systemd order outcome | einkd after getty/net/resolv/plat; no boot/getty panel access |

No C, no copied CDDL/GPL, and no new donor-derived table.

## 9. Open inputs

* Owner-approved plain-HTTP content URL.
* Product-side SPI0 handle/namespace grant and `platd` GPIO level call (or kernel SPI resource
  driver); see section 4.
* SD2 RW UFS root and SD3 first real-card boot log.
* E5 panel photo and board serial transcript.
