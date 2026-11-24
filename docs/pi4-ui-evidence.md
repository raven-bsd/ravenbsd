<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 HDMI + USB keyboard board evidence (UI1)

Status: **HOST PREP COMPLETE; BOARD EVIDENCE BLOCKED.** No Pi 4, HDMI monitor, USB keyboard or
UART adapter is attached to this host, so no board log or photo exists and none is claimed. The
host checks and the provider handoffs below are the first failure.

This page is the owner runbook and the marker contract for the UI1 gate. The helper is
`scripts/pi4-ui-evidence.sh`; it never generates a log or a photo.

## 1. Scope and current provider status

| Path | Status | Evidence |
|---|---|---|
| Kernel early HDMI text | HDMI1 `DONE` (host codec/blitter, firmware config); **board UNVERIFIED** | `docs/hdmi-console.md`, `cargo nextest run -p raven-fb` 21/21 |
| **HDMI userspace login/getty** | **BLOCKED**: `consd` opens `fb:console`, but no userspace server registers an `fb:console` provider. HDMI therefore keeps the last kernel text; `getty`/`login` stays on UART. | `userland/servers/consd/src/main.rs:365`; no `Server::register("fb"...` in `userland/` |
| UART console | CON1 `DONE` (host multiplexer); **board UNVERIFIED** | `cargo nextest run -p raven-consd -p raven-ttyd` 18/18 |
| USB keyboard driver/queue | USB1 `DONE` (host/synthetic `pi4-usb`); **board UNVERIFIED** | `docs/pi4-usb.md`, `pi4-usb` qtest |
| **Keyboard into consd** | **BLOCKED**: `consd` opens `kbd:console`; `usb-hid` serves `kbd:` (root only), and `usb-hid` rejects the `console` path. The xHCI driver pushes to `kbd:`, so the bytes never reach the console. | `userland/servers/consd/src/main.rs:142`; `userland/drivers/usb-hid/src/main.rs:133` |
| brush prompt/colors | Profile/brushrc staged (`TERM=vt100`, Raven Violet PS1); **board UNVERIFIED** | `etc/profile`, `etc/brush/brushrc` |
| HDMI photo + UART fallback | **BLOCKED on owner hardware** | no adapter/board in this session |

The two provider entries are genuine integration defects, not host-check noise. A board run before
they are fixed can prove the kernel HDMI text and the UART login, but not the HDMI login or the USB
keyboard path. Hand the first failure to HDMI1 (fb provider), USB1/CON1 (kbd:console) or the owner
hardware.

## 2. Host preflight

Build the current SD image and record its exact hash:

```sh
RAVEN_RPI_FIRMWARE=/home/bo/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot \
  SOURCE_DATE_EPOCH=1700000000 \
  cargo xtask image --arch aarch64 --platform pi4 --profile pi4-rootfs
sha256sum build/aarch64/ravenbsd-pi4-sd.img
```

Host checks (no serial port, no board):

```sh
sh scripts/pi4-ui-evidence.sh --check
sh scripts/pi4-ui-evidence.sh --check --strict   # expected exit 2 while a blocker remains
sh scripts/pi4-ui-evidence.sh --plan
make check-fast
```

`--check` runs `scripts/pi4-release.sh --check` (GPT/FAT/UFS, release initfs, DTB) and
`scripts/pi4-serial.sh --check` (FAT manifest), records the commit and image SHA, and prints the
current hardware/provider blockers. `--strict` exits 2 if any blocker remains; today it reports:

```
pi4-ui-evidence: board BLOCKED: no /dev/ttyUSB* or /dev/ttyACM* adapter
pi4-ui-evidence: provider BLOCKED: no userspace fb:console provider; HDMI keeps kernel text and login stays on UART
pi4-ui-evidence: provider BLOCKED: consd opens kbd:console while usb-hid serves kbd: (root)
```

The host image checks pass; the board gate does not. No fixture substitutes for the owner capture.

## 3. Owner board sequence

Hardware: Pi 4 (record model/revision), HDMI monitor and cable, USB keyboard, SD card, 3.3 V
USB-UART adapter (GPIO14 TX, GPIO15 RX, GND; never connect adapter VCC).

1. **Flash and insert (owner only).** Choose the SD block device from `lsblk`; it must not be
   mounted and must not contain `/`. The installer re-checks root/mount/size and refuses unsafe
   targets.

   ```sh
   sudo sh scripts/pi4-install.sh --flash /dev/sdX --yes \
     -i build/aarch64/ravenbsd-pi4-sd.img
   ```

   Insert the card, connect HDMI + USB keyboard + UART, then power on.

2. **Capture the UART log.** Run this in the foreground and stop with Ctrl-C; keep the footer.

   ```sh
   sh scripts/pi4-ui-evidence.sh --capture -d /dev/ttyUSB0 \
     -l build/pi4/ui1-serial.log
   ```

   The equivalent release capture is:
   ```sh
   sh scripts/pi4-release.sh --device /dev/ttyUSB0 --log build/pi4/ui1-serial.log \
     -i build/aarch64/ravenbsd-pi4-sd.img --no-check
   ```
   Do not use the release `evaluate_log` as the UI1 gate: it forbids `Login incorrect`, while UI1
   requires the wrong-password retry.

3. **HDMI + keyboard session.** With the HDMI monitor attached:
   - kernel banner/kernel text on the monitor (HDMI1 early console);
   - login prompt on HDMI once the `fb:console` provider lands; today the prompt stays on UART;
   - wrong password once, then the correct login:
     ```text
     login: root
     Password: wrong
     Login incorrect
     login: root
     Password: raven
     ```
   - at `raven#`:
     ```text
     echo UI1-HDMI-OK
     echo "UI1-TERM=$TERM"
     echo "UI1-PS1=$PS1"
     ls --color=always /bin
     ping 8.8.8.8
     ```
   - record the monitor model, native resolution and the negotiated mode (HDMI1 fixed DMT
     1024x768 60 Hz; record if the monitor rejects it);
   - take a photo/screenshot of the monitor showing the prompt and at least one command output.
     Record the path and SHA-256.

4. **UART fallback.** In the same serial capture, log in over UART after the HDMI/keyboard
   session:
   ```text
   login: root
   Password: raven
   raven# echo UI1-UART-OK
   raven# echo "UI1-TERM=$TERM"
   ```
   This proves the UART recovery console still works while HDMI/keyboard are attached.

5. **Verify the capture.**
   ```sh
   sh scripts/pi4-ui-evidence.sh --verify --log build/pi4/ui1-serial.log \
     --photo /path/to/monitor.jpg
   ```
   The verifier requires the markers below and a non-empty photo; it does not judge the photo
   content. A missing marker fails and prints the first missing gate.

## 4. Evidence metadata

Fill this from the real capture; never guess.

```text
UI1 Pi 4 HDMI + USB keyboard evidence
=====================================
Date (UTC):
Commit:                       # git rev-parse HEAD
Image:                        # path and size
Image SHA-256:                # sha256sum build/aarch64/ravenbsd-pi4-sd.img
Board model/revision:         # from the board label/log
HDMI monitor:                 # model, native resolution, cable
Negotiated mode:              # 1024x768@60 (DMT 16) or the actual mode
USB keyboard:                 # model and connection (VL805 port)
SD card:                      # model and capacity
UART adapter and device:      # e.g. CP2102 /dev/ttyUSB0, 115200 8N1
Serial log:                   # path + SHA-256
Photo/screenshot:             # path + SHA-256
Wrong-password retry:         # Login incorrect, then successful root/raven
HDMI marker:                  # UI1-HDMI-OK
UART fallback marker:         # UI1-UART-OK
Brush env/colors:             # UI1-TERM=... / UI1-PS1=... plus visual prompt
ls /bin:                      # brush, ls, uname, ping present
Ping:                         # 8.8.8.8 reply or the documented gateway fallback
First failing line:           # exact line if the run stopped early
Result:                       # PASS / FAIL / BLOCKED
```

## 5. Required capture markers

`scripts/pi4-ui-evidence.sh --verify` checks these literal markers:

| Gate | Marker |
|---|---|
| kernel | `RavenBSD kernel` |
| login | `login:` |
| wrong-password retry | `Login incorrect` |
| prompt | `raven#` |
| HDMI session | `UI1-HDMI-OK` |
| UART fallback | `UI1-UART-OK` |
| brush environment | `UI1-TERM=` |
| brush prompt | `UI1-PS1=` |
| shell listing | `ls /bin` (and `brush` in the output) |
| ping target | `8.8.8.8` |
| ping reply | `bytes from` |

`panic`/`EXCEPTION` are forbidden. `Login incorrect` is **expected** once for the retry and is not
a failure for this gate. A missing `UI1-HDMI-OK`/`UI1-UART-OK` is a missing session, not a text
format issue.

## 6. Failure handling

* Preserve the first failing log and photo; do not overwrite them with a later run.
* If the monitor shows only kernel text and the login stays on UART, first check the
  `fb:console` provider handoff (HDMI1/UI1), then the monitor/cable/mode.
* If the USB keyboard types nothing, first check the `kbd:console` vs `kbd:` handoff
  (USB1/CON1), then the VL805/xHCI probe and the keyboard.
* If `ping 8.8.8.8` fails, record the DHCP lease/default gateway and try the gateway ping; mark
  external ICMP as blocked rather than a keyboard/HDMI failure.
* If the board does not reach `login:` over UART, record the first kernel/block/GPT/UFS line and
  hand it to the SD3/storage owner.
* No fake log, no generated photo, no borrowed transcript. A partial real log is a failure report,
  not a pass.

## 7. Donor reconciliation (DR-57 / DR-52)

UI1 adds only the host-preparation script and this runbook; it consumes the HDMI1/USB1/CON1
designs. No donor code was read for new implementation and none was copied.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| Framebuffer text console | no Pi firmware framebuffer; design only | FreeBSD `bcm2835_fb.c`, `bcm2835_mbox*` (BSD-2); NetBSD `bcm2835_genfb.c` (BSD-2) | `simpledrm`/fbcon outcomes; read-only | HDMI1 first-party code; UI1 only verifies the board |
| USB HID keyboard | Illumos `clients/hid/hid.c` split (CDDL, design only) | NetBSD `ukbd.c` boot report/modifiers (BSD-2) | `drivers/hid/usbhid` read-only | USB1 first-party `raven-usb-hid`/`usb-hid`; UI1 verifies |
| Console multiplexing | `sdev_vtops.c` console/terminal split (CDDL, design only) | NetBSD `wsdisplay.c`/`wskbd.c` (BSD-2) | `drivers/tty/vt`+fbcon read-only | CON1 first-party multiplexer; UI1 verifies |
| Board evidence | no board in this ticket | no permissive board log copied | no borrowed transcript | Owner hardware only; **BLOCKED** here |

Donor revisions: Illumos `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`; NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`; FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`;
Linux `adc218676eef25575469234709c2d87185ca223a`. No CDDL/GPL code was copied, translated or
closely paraphrased.

## 8. Blockers and owners

| Blocker | Owner |
|---|---|
| No board, HDMI monitor, USB keyboard or UART adapter attached | owner hardware; **BLOCKED** |
| No userspace `fb:console` provider; HDMI login/getty cannot appear | HDMI1/UI1 |
| `consd` opens `kbd:console`; `usb-hid` serves `kbd:` root; keyboard bytes do not reach consd | USB1/CON1 |
| Monitor mode/photo and real serial transcript missing | owner; SD3/UI1 board run |

Until the provider entries and the owner hardware are available, the honest result is **BLOCKED**;
the host image checks and the runbook are the delivered UI1 preparation.
