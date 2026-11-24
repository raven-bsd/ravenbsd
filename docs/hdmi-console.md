<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 HDMI text console (HDMI1)

Status: **host codec/blitter, firmware configuration and the K386 qtest OBSERVED; the Pi 4
board output is UNVERIFIED and handed to UI1.** This is a firmware linear display buffer
rendered as a text console. It is not GPU/DRM, has no acceleration and makes no claim about a
monitor result.

## 1. Scope and boundary

The owner requires a monitor plugged into the Pi 4 to show boot/login text. HDMI1/FB1 deliver:

* a deterministic firmware display buffer in `config.txt`;
* `crates/raven-fb`, a host-tested mailbox property codec, pixel formats, 8x16 text blitter,
  scrolling and cursor;
* a Pi 4 kernel early console that asks the firmware for the display buffer, mirrors every kernel
  log line to it and keeps the UART output unconditional;
* FB1's `DEBUG_WRITE` mirror: the consd UART sink used by getty/login also calls
  `arch::aarch64::serial::write_bytes`, which mirrors the same bytes to the HDMI text console
  before the unconditional UART write. This is the login-output path; no second console sink is
  needed;
* the existing `consd` `fb:console` output sink remains an optional userspace fan-out point
  (CON1); a userspace display provider is no longer required for the kernel mirror.

The kernel early console is the boot path. The HDMI screen shows the same byte stream as UART:
kernel banner/log lines and, through FB1, getty/login output. A failed mailbox or display buffer
leaves UART-only operation and the boot/login gate unchanged. This document does not fake a
monitor transcript; the physical panel and USB keyboard are UI1 board evidence.

E-Ink keeps its `Canvas` terminology in `raven-eink`. The HDMI path uses **display buffer** /
**HDMI console** and does not reintroduce a Canvas-style model.

## 2. `config.txt`

`xtask/src/pi.rs` writes the boot partition configuration. HDMI1 adds the firmware's own
`framebuffer_*` configuration keys (the firmware spelling; kept as data-sheet names):

```
framebuffer_width=1024
framebuffer_height=768
framebuffer_depth=32
framebuffer_ignore_alpha=1
hdmi_force_hotplug=1
hdmi_group=2
hdmi_mode=16
disable_overscan=1
```

`hdmi_group=2`, `hdmi_mode=16` is the DMT 1024x768 60 Hz mode, so the firmware mode and the
display property request agree. `hdmi_force_hotplug=1` keeps the mode when no monitor is sensed at
power-on. The existing `enable_uart=1`, `uart_2ndstage=1`, `dtoverlay=disable-bt`,
`init_uart_clock=48000000` and `core_freq=250` lines are unchanged: UART remains the primary boot
and recovery console. `hdmi_safe=1` is deliberately not used because it forces 640x480 and would
contradict the fixed display request.

## 3. Mailbox property flow

The ARM core writes a 16-byte-aligned property buffer physical address to mailbox channel 8 and
reads the same address from the channel-8 FIFO. The Pi 4 mailbox frame is at physical
`0xFE00_B880`; the kernel uses `0x00` (read), `0x18` (status full/empty) and `0x20` (write) through
the HHDM device window.

Message layout (`crates/raven-fb/src/mailbox.rs`):

```
header   size (bytes), code (0)
tag      tag id, value-buffer bytes, request bytes, values/padding
...
end      0
```

HDMI1 sends one message with this fixed order:

| Tag | Id | Request | Response |
|---|---|---|---|
| `SET_PHYSICAL_W_H` | `0x00048003` | width, height | width, height |
| `SET_VIRTUAL_W_H` | `0x00048004` | width, height | width, height |
| `SET_DEPTH` | `0x00048005` | 32 | 32 |
| `SET_PIXEL_ORDER` | `0x00048006` | `1` (RGB) | `1` |
| `ALLOCATE_BUFFER` | `0x00040001` | 4096 alignment | bus base, size |
| `GET_PITCH` | `0x00040008` | none | bytes per row |

The firmware clears the header code to `0x8000_0000` and sets bit 31 of each tag's value length.
A tag without bit 31 is a rejected tag; a malformed/duplicate/missing tag is rejected as a whole.
`ALLOCATE_BUFFER` is the allocation form used here; it is the same property that older
documentation calls `GET_FRAMEBUFFER`. The decoder also validates that `pitch * height` fits the
returned allocation.

## 4. Address mapping and cache/barrier rules

The Pi 4 firmware returns the display buffer as a VideoCore bus address in the legacy master view.
For BCM2838 SDRAM the uncached alias starts at `0xC000_0000`; the kernel subtracts that base to get
the ARM physical address, then adds the HHDM base. The Pi 4 entry stub maps the low 2 GiB of RAM
through the HHDM, so the allocation must stay below `0x8000_0000`; otherwise the console logs a
fallback and stays on UART.

Both the property buffer and the display buffer are normal cacheable SDRAM:

1. Build the property message in a static `#[repr(align(16))]` buffer.
2. Clean that buffer to the point of coherency (`dc cvac`) and execute `dsb sy`, so the VideoCore
   sees the message bytes.
3. Ring the mailbox doorbell and poll the status register with a bounded count.
4. Execute `dsb sy` and invalidate the property buffer (`dc ivac`) before decoding the response.
5. After drawing, clean each changed pixel-row range (`dc cvac` over a 64-byte-line span) and
   `dsb sy` before the next scanout. The console returns an inclusive dirty-row range, so the
   kernel does not clean the whole 3 MiB for every log line.
6. Never use a barrier as a substitute for cache maintenance: the doorbell and the display engine
   are DMA masters.

The cache-line size is 64 bytes (Cortex-A72). The kernel reentrancy guard makes a concurrent panic
line skip the HDMI mirror instead of racing the normal console writer; UART output is never
skipped.

## 5. Fallback

| Failure | Behaviour |
|---|---|
| Missing mailbox response / timeout | `fb: mailbox timeout; UART only`, no panic |
| Non-success or malformed property response | `fb: response rejected: ...`, no panic |
| Unsupported depth | `fb: depth <n> unsupported`, no panic |
| Buffer outside the HHDM RAM window | `fb: display buffer outside the mapped RAM window`, no panic |
| Allocation smaller than pitch × height | `fb: response rejected: AllocationTooSmall`, no panic |
| Any later console error | the line was already written to UART; the HDMI write is dropped |

The kernel log writer writes the UART first, then mirrors. A monitor is therefore an extra output,
never a boot dependency.

## 6. Host evidence

`cargo nextest run -p raven-fb` (21 tests):

| Area | Assertions |
|---|---|
| mailbox encode | exact 120-byte sequence, tag ids, lengths, terminator |
| mailbox decode | successful response, firmware error, missing response bit, missing tag, short allocation |
| request validation | short buffer, unsupported depth |
| pixels | depth→format, bytes per pixel, black/white per depth |
| font | all printable ASCII cells, known glyph, `?` replacement |
| text | clear, glyph, newline, scroll, wrap, backspace, 16/8-bit white, short screen |

The Pi 4 kernel (`RAVEN_PI4=1 cargo check -p raven-kernel --target
aarch64-unknown-none-softfloat`) compiles the shared crate sources through `#[path]`, so the kernel
has no new Cargo dependency and the host tests exercise the same files.

## 7. Board steps for UI1 (UNVERIFIED)

1. Build `cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release` and flash the
   image. Keep the UART connected.
2. Boot with an HDMI monitor attached and capture the UART transcript. Expect the usual
   `RavenBSD kernel`, `platform:`, `mem:` and login lines on UART.
3. Expect the same kernel lines on the monitor after a short firmware hand-off, starting with the
   `fb: 1024x768x32 pitch=...` line. Capture the monitor with a photo/video and record the panel
   model, native resolution and cable.
4. Boot once without a monitor and once with a monitor hot-plugged after power-on; UART must be
   byte-identical in both cases and the boot/login gate must not regress.
5. FB1's kernel `DEBUG_WRITE` mirror carries `login:`/getty output to HDMI; the real
   panel/keyboard login (and any userspace `fb:console` provider) is UI1 evidence, not HDMI1.

## 8. DR-57 / DR-52 reconciliation

The mailbox tags, response-bit protocol and allocation order were studied in the permissive BSD
donors; no CDDL/GPL code was read or copied. Donor revision: NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`, FreeBSD
`133344405f532bd54ff3629a77d4c4e062f01fdb`.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| Mailbox property protocol | No Pi firmware mailbox; no equivalent service | FreeBSD `sys/arm/broadcom/bcm2835/bcm2835_mbox.c` + `bcm2835_mbox_prop.h` (BSD-2-Clause); NetBSD `bcm2835_mbox.c`/`bcm2835_mbox_subr.c` (BSD-2-Clause) | `drivers/firmware/raspberrypi` property API, read-only | Clean-room Rust codec from the documented tag sequence and the BSD error/response rules; no code copied |
| Display buffer allocation | No firmware display buffer | FreeBSD `bcm2835_fb.c` `bcm2835_mbox_fb_init` (BSD-2-Clause) | `simpledrm`/`simpledrmfb` outcome only | Six-tag one-message request; `ALLOCATE_BUFFER` + `GET_PITCH`; no virtual offset/alpha tags because the contract fixes them at defaults |
| Bus address translation | No VideoCore bus | FreeBSD `bcm2835_vcbus.c/h` (BSD-2-Clause) | `linux-rpi` legacy master view, read-only | `vcbus - 0xC0000000` for BCM2838 SDRAM, validated against the HHDM window |
| Text console/blitter | Illumos `wscons`-equivalent terminal is not applicable | NetBSD `bcm2835_genfb.c` (BSD-2-Clause) attaches `genfb`; the 8x16 glyph data is NetBSD `wsfont/bold8x16.h` (public-domain data, BSD-2 file) | fbcon outcome only | First-party blitter/scroll/cursor; glyph table reused from the already-proven E1d font with the same donor notice |
| Cache ownership | No firmware DMA display buffer | FreeBSD `pmap_mapdev` maps the buffer uncached (BSD-2-Clause) | DMA coherent mapping outcome | CPU HHDM mapping is cacheable, so explicit `dc cvac`/`dc ivac` + `dsb sy` around mailbox and scanout |
| `consd` sink | SMF/console fan-out outcome | CON1 first-party `ConsoleOutput` in `userland/servers/consd` | systemd-logind/seat outcome only | FB1 kernel `DEBUG_WRITE` mirror is the primary login path; the `fb:console` sink stays optional for a userspace provider |

## 9. Open inputs

* The live init presentation of the styled screen is K374. The K386 qtest asserts the styled
  screen through the target probe's serial mirror (marked `hdmi-console: styled screen`) and the
  host PPM; the live init's concise boot report remains in the same transcript and is not an
  HDMI-console gap.
* Real-panel mode/timing evidence if the monitor rejects DMT 1024x768 60 Hz; the fallback is a
  different fixed `hdmi_mode`.
* A real monitor photo, USB-keyboard login and panel/cable record belong to UI1; the K386 qtest
  never substitutes for them.

## 10. K386 qtest (`hdmi-console`)

`tests/qtest/cases/hdmi-console.toml` boots a minimal test initfs with the production
`init`/`getty`/`login` path and a target probe, for both instruction sets:

```sh
timeout 600 cargo xtask qtest --case hdmi-console --arch aarch64
timeout 600 cargo xtask qtest --case hdmi-console --arch x86_64
```

The fixture (`tests/fixtures/hdmi-console/`) is deliberately smaller than the release image: it
uses the production service binaries, getty/login and the rsh login shell, plus a read-only
account fixture. That isolates the console/login path from release-image build time; it does not
disable a service or replace the product getty/login. `getty` resolves the root account to
`/bin/brush`, so the fixture stages the product `raven-rsh` binary at that path; brush itself,
completion and the release profile stay covered by `brush-login` and `login-retry`. The
HDMI-console contract is the console output path, not a second brush test.

What the case proves:

| Claim | Artifact |
|---|---|
| Firmware mode is 1024x768x32, from the actual `xtask/src/pi.rs` `CONFIG_TXT` literal | `build/fixtures/hdmi-console/config.txt`, `host.log` |
| `raven-fb` renders the exact 8x16 glyph, scrolls and keeps the cursor | target `hdmi-console: blit/scroll ok`; host `build/qtest/hdmi-console.ppm` |
| Malformed/failed mailbox replies return errors; the kernel's `fb.rs` failure paths stay `UART only` and contain no panic/unwrap | target `hdmi-console: mailbox fallback ok`; host `host.log` source checks |
| The styled screen (title, 80-column rules, timed rows, `Welcome`) is on the serial stream the Pi 4 kernel mirrors | target `hdmi-console: styled screen` block in `build/qtest/hdmi-console.log` |
| Boot banner and the real `login:` prompt are on the same stream | `build/qtest/hdmi-console.log`, `hdmi-console.json` |
| Login `root`/`raven`, shell prompt, one command and clean `init exited (0)` | same serial log |

The QEMU `virt` harness has no Pi 4 firmware mailbox and no panel, so there is no hardware
framebuffer capture. The honest chain is: the serial log is the byte stream that
`arch::aarch64::serial::{write_str,write_bytes}` mirrors to `fb::write_text` on a Pi 4 build;
the target probe prints the styled screen and runs the same `raven-fb` blitter in the guest; and
the host check renders that screen into `build/qtest/hdmi-console.ppm` at 1024x768x32. The
`hdmi-console: styled screen` marker separates the probe's lines from the init's concise boot
report; the real getty owns `login:`. The PPM is a synthetic blitter render, not a photo of a
monitor. A real monitor and USB keyboard login remain UI1 board evidence, and no
physical-monitor claim is made here.
