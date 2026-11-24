<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Console multiplexer (CON1)

Status: **host-tested only**. No hardware/QEMU claim is made by CON1. UART remains the recovery
console; HDMI text output and USB keyboard input are wired through the backends below and owned by
HDMI1/USB1/UI1 for board evidence. FB1 adds the getty/login output path: the kernel mirrors the
`DEBUG_WRITE` stream to the Pi 4 HDMI text console, so the existing `consd` UART sink also appears
on HDMI without a userspace display provider.

## Contract

`getty` and `login` continue to open `tty:console`. The scheme still reports a character device,
implements canonical/raw mode with echo through `raven-tty-core`, and keeps the existing
`SET_MODE`, `FLUSH`, `GET_WRITE`/`SET_WRITE` CALL subset. CON1 only changes what feeds the
discipline and where the output is fanned out.

```
kbd:console (USB1) ─┐
                    ├─> Multiplexer -> Discipline -> ready lines -> tty:console readers
UART DEBUG_READ ────┘                         │
                                              └─ echo/writes -> UART DEBUG_WRITE
                                                             -> kernel HDMI text mirror (Pi 4)
                                                             -> fb:console (optional userspace sink)
```

## Backends

| Backend | Trait | Wiring | Owner |
|---|---|---|---|
| UART debug | `UartInput` / `UartOutput` | `DEBUG_READ` / `DEBUG_WRITE` via `raven-rt` | base / recovery |
| USB boot keyboard | `ConsoleInput` over `kbd:console` | byte stream published by the USB HID service | USB1 |
| HDMI/framebuffer text | `ConsoleOutput` over `fb:console` | optional byte sink; the kernel `DEBUG_WRITE` mirror is now the primary getty/login path | HDMI1/FB1 / UI1 |

The keyboard reader thread exits silently when `kbd:console` is absent; the framebuffer sink is
added only when `fb:console` opens. Both failures leave UART-only operation intact.

## Ordering, echo and fan-out

1. Input sources are polled in registration order: UART first, then keyboard. Bytes from each
   source preserve that source's order; sources are not reordered inside one poll.
2. Every source feeds one `Discipline`. Canonical mode delivers complete lines (with `\n`), handles
   erase/kill/`^C`/`^D`, and emits echo for every live output. Raw mode delivers bytes unmodified
   and emits no echo.
3. Output fan-out writes the same bytes to each live sink exactly once. A prompt written by
   getty/login (`login: `) is therefore present once on UART and once on the framebuffer, not a
   duplicate per input source.
4. A short or full sink retains the unsent bytes in its per-sink queue up to `MAX_PENDING`
   (`64 KiB`); the next flush retries. A closed/IO sink is retired and the remaining sinks continue.
5. When every input source ends, pending `READ`s return EOF. When every output sink is gone,
   `WRITE` returns `EIO`; an input-less but output-live console stays writable.

No process-server semantics are involved: pids, wait, signals and credentials remain kernel
objects. `consd` is still the `tty:` scheme server.

## FB1: getting login output to HDMI

`getty`/`login` write to `tty:console`; `consd` sends those bytes through its `UartOutput`, which
calls `raven_rt::debug_write`. On a Pi 4 build the kernel `DEBUG_WRITE` path calls
`arch::aarch64::serial::write_bytes`; that function now mirrors the same byte stream to the
existing `arch::aarch64::fb` text console **and** writes the UART unconditionally. This is the
preferred FB1 path: no duplicate sink, no GPU/DRM driver, and the existing `fb:console` scheme
sink remains optional for a future userspace provider.

The mirror is only active after the HDMI console has successfully queried the firmware display
buffer. A missing mailbox/display leaves the UART path untouched; the boot/login serial gate does
not regress. The panel/E-Ink path is not involved.

`scripts/pi4-ui-evidence.sh --check --strict` treats the kernel mirror as the HDMI login provider
and fails strict mode only for a missing provider, not for the absent board/photo. A board login
still requires UI1's real HDMI + USB-keyboard evidence.

## Board evidence hook (UI1)

CON1 defines the wire points, not the hardware behaviour:

* **HDMI1** publishes an `fb:console` text byte sink; UI1 records the monitor output, boot text and
  login prompt on the HDMI console.
* **USB1** publishes `kbd:console` ASCII/control bytes from the boot keyboard; UI1 records a real
  USB keyboard login.
* **UART** remains available as the recovery console and must stay byte-identical for the serial
  qtests.

Board evidence is requested by the HDMI1/USB1/UI1 tickets (`hdmi-console`, `pi4-usb`); CON1's
acceptance is the host matrix below only.

## Host test matrix

`cargo nextest run -p raven-consd` (10 tests):

| Case | Assertion |
|---|---|
| UART-only | canonical line delivered, echo on UART |
| keyboard-only | same discipline path from `kbd:` |
| both inputs | source order preserved, one discipline |
| echo/canonical/raw | echo, backspace, raw passthrough |
| fan-out | one copy per sink, no duplicate prompt |
| repeated prompts | two prompts stay two, not four |
| backpressure | bytes retained until the sink drains |
| output loss | failed sink retired, other continues |
| input loss | failed source retired; EOF only when all end |
| interrupt/flush | `^C` and `FLUSH` visible to the scheme reader |

## Donor reconciliation (DR-57/52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| multiplexed console input/output | `sdev_vtops.c` / `vt`/console daemon separates the console device from the terminal emulator; CDDL design only | NetBSD `sys/dev/wscons/wsdisplay.c` and `wskbd.c` (`wscons` + `wsmux` multiplex display/keyboard devices); BSD-3/4-clause reference, no copy. FreeBSD vt/ukbd is not present in the sparse donor checkout | Linux `drivers/tty/vt` + `fbcon` route the console to VT/framebuffer; read-only outcome | Original Rust `ConsoleInput`/`ConsoleOutput` traits and a host-tested multiplexer; no donor code copied |
| line discipline | Illumos tty discipline is in `ldterm`; design only | NetBSD `sys/dev/wscons` passes bytes to the tty layer; BSD reference | Linux n_tty canonical/echo semantics; read-only | Reuse the existing `raven-tty-core` discipline; CON1 does not duplicate it |
| device loss/fallback | vt/console detach paths retire the backend; design only | wscons detach removes the muxed device; BSD reference | fbcon/VT unbind keeps other consoles alive; read-only | Retire failed sinks/sources; UART-only fallback, no fake device |

Donor revisions: `netbsd 36eddebcc36cb0775aa2962005b9c05419a6ff01`,
`illumos-gate 586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`,
`linux adc218676eef25575469234709c2d87185ca223a`; FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`
(sparse checkout has no `sys/dev/vt`). No CDDL/GPL code was copied or translated.

## Limits

* No HDMI framebuffer driver, USB xHCI binding or board claim in CON1.
* `fb:console` is a byte sink contract; glyph rendering, resolution, KMS/DRM and console resize
  belong to HDMI1/UI1.
* `kbd:console` bytes are already ASCII/control bytes; HID decode and keymaps belong to USB1.
* No hotplug beyond the backend's open/read/write failure retirement.
