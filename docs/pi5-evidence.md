<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 5 boot image and serial evidence kit (R8)

Status: host-side kit only. The profile, the FAT manifest check, the fixture and the profile
smoke test are OBSERVED on the development host; no Raspberry Pi 5 board log exists. Everything
that depends on the EEPROM bootloader, the firmware-selected kernel or the UART wiring is
UNVERIFIED until the owner runs the capture on a board.

B54 documented the Pi 5 boot path and the `raven-pi5` host model. R8 makes the evidence
repeatable: one profile, one FAT manifest, one timestamped capture script and one template that
records the commit, image hash and first failing line.

## 1. Profile and image boundary

`profiles/pi5.toml` extends `base` and uses the proposed `console=ttyAMA0 loglevel=info` command
line. A generic aarch64 image with that profile can be built and booted under QEMU:

```
cargo xtask image --arch aarch64 --profile pi5
```

The Pi 5 **FAT boot image** itself is not produced by that command. B54 deliberately left the
`xtask/src/image.rs`/`cli.rs` dispatcher and the Pi 5 load-address question open; R8 keeps the same
boundary. The manifest checker validates a Pi 5 FAT image produced by the future platform builder
or by the owner's staging step. It never guesses the `kernel_2712.img` load address or the entry
exception level.

The host fixture `tests/fixtures/pi5-image/build.sh` builds a small synthetic FAT image at the
1 MiB partition offset, runs `scripts/pi5-serial.sh --check`, and exercises the same manifest code
without a board or a real firmware image. The real firmware files stay in the read-only donor
checkout and are never committed.

## 2. FAT manifest

`tests/fixtures/pi5-image/manifest.txt` lists the boot partition files and the check for each:

| File | Requirement |
|---|---|
| `config.txt` | `arm_64bit=1`, `kernel=kernel_2712.img`, `device_tree=bcm2712-rpi-5-b.dtb`, `enable_uart=1`, `uart_2ndstage=1` |
| `kernel_2712.img` | at least 64 KiB; the first 16 bytes are not zero |
| `bcm2712-rpi-5-b.dtb` | at least 1 KiB; FDT magic `0xd00dfeed` |
| `start4.elf` | at least 1 MiB |
| `fixup4.dat` | at least 1 KiB |
| `LICENCE.broadcom` | present |

The manifest is a **staging check**, not a hardware claim. It verifies that the selected kernel
name is `kernel_2712.img` (not the `kernel8.img` fallback), that the DTB is an FDT and that the
config selects both. The exact Pi 5 EEPROM file set, the kernel load address, the entry EL and the
UART clock remain UNVERIFIED (B54 section 6).

## 3. Prerequisites

* Rust and the repository toolchain (`make doctor`).
* The read-only firmware checkout `ravenbsd-resources/rpi-firmware/boot`; never committed.
* `mtools` (`mformat`, `mcopy`, `mtype`) for the fixture and manifest check.
* `stty`, `sha256sum` (or `shasum`) and a 3.3 V USB-UART adapter for capture.
* `picocom` for a clean capture; the script falls back to `cat` and says so.
* Do **not** connect the adapter's VCC to the Pi. The JST-SH debug connector's GND/TX/RX order and
  the UART10 input clock are UNVERIFIED (see `docs/pi5-boot.md` section 3).

If the clone and the resources are on different paths, pass `RAVEN_RPI_FIRMWARE` to the image
builder. The manifest script itself only reads the image.

## 4. Manifest check

```
sh tests/fixtures/pi5-image/build.sh                 # synthetic host fixture
scripts/pi5-serial.sh --check -i build/fixtures/pi5-image/pi5-boot.img
scripts/pi5-serial.sh --check -i build/aarch64/ravenbsd-pi5.img
```

`--check` reads the FAT partition with `mtype` and prints one line per file, then
`pi5-manifest: PASSED`. A missing file, a too-small file, a zero kernel prefix, a non-FDT DTB or a
missing config line fails with the first bad path and exits 1. `sh -n scripts/pi5-serial.sh` is the
syntax gate.

## 5. Capture

Connect the adapter and run:

```
scripts/pi5-serial.sh -d /dev/ttyUSB0 -i build/aarch64/ravenbsd-pi5.img
```

The script:

1. runs the manifest check and fails before opening the device if a boot file is missing;
2. writes a header with the UTC start, host, commit, image path, full image SHA-256, device and
   manifest into `build/pi5/<image-sha256>.log`;
3. configures the port with `stty` (`raw`, 8N1, no CRTSCTS, no echo) and captures with `picocom`
   (or `cat`);
4. prefixes every line with a UTC timestamp and appends to the log;
5. writes a `# stopped:` footer on Ctrl-C or normal exit.

Options: `-l` selects a log path, `-b` the baud rate, `--no-check` skips the manifest step, and
`-c` runs only the manifest check. The script is `sh -n` clean and is host-tested by the
`pi5-image` qtest pre step.

## 6. Evidence template

Copy this block into the report or the ticket's evidence note and fill every line from the log.
Leave a line as `NOT OBSERVED` rather than guessing.

```text
Pi 5 boot evidence
==================
Date (UTC):
Commit:                       # git rev-parse HEAD
Image:                        # path and size
Image SHA-256:                # sha256sum build/aarch64/ravenbsd-pi5.img
Firmware revision:            # rpi-firmware entry in donors.lock
Board model/revision:         # from the board label / log
SD card:                      # model and capacity
UART adapter and device:      # e.g. CP2102 /dev/ttyUSB0, 115200 8N1
Log file:                     # build/pi5/<image-sha256>.log

Manifest check:               # PASSED / first failing file
Kernel banner:                # exact first RavenBSD line
Memory map:                   # exact "mem:" line
UART10 / console:             # selected console, clock if observed
GIC/timer:                    # distributor/CPU interface, timer source
PCIe/RP1:                     # link status / RP1 enumeration (B55)
First failing line:           # exact log line, or "none"
Result:                       # PASS / FAIL (first failure)
```

## 7. Board steps (UNVERIFIED)

No Pi 5 board run was performed for R8. None of these steps has been executed:

1. Produce a Pi 5 FAT boot image with the future platform builder (or the owner's staging step)
   and run `scripts/pi5-serial.sh --check` on it; record the image SHA-256 and firmware revision.
2. Flash the image to the SD card, connect the 3.3 V UART adapter at 115200 8N1, and power the
   board.
3. Capture the whole boot with `scripts/pi5-serial.sh`; do not edit the log.
4. Record the kernel banner, memory map, UART/console, GIC/timer and the first PCIe/RP1 line in
   the template.
5. If the boot stops, record the first failing line; do not claim later steps from a partial log.

The Pi 5 EEPROM bootloader's file selection, `kernel_2712.img` load address/entry EL, the UART10
clock, the connector pin order and the RP1 firmware/PCIe path all remain UNVERIFIED. A successful
QEMU boot of the generic `pi5` profile is a profile regression, not board evidence.

## 8. Relationship to B54 and B55

* B54 owns the Pi 5 platform model (`crates/raven-pi5`), the DTB fixtures, the profile skeleton and
  the host-side image layout. R8 reuses that descriptor and adds the evidence kit.
* B55 owns the RP1 firmware load and PCIe/RP1 bring-up. No RP1 blob is present, linked or
  redistributed by R8.
* The `pi5-image` qtest runs the host manifest fixture and boots the generic aarch64 profile under
  QEMU; it does not run a Pi 5 board and does not claim one.

## 9. Provenance

The profile, manifest, script, fixture and template are first-party BSD-3-Clause. No firmware or
DTB blob is committed. The real Pi 5 firmware/DTB are user-supplied add-on files read from the
read-only `rpi-firmware` checkout at build time; the donor revision is recorded in
`ravenbsd-resources/donors.lock` and must be copied into the evidence block when the owner builds
the image. The synthetic fixture uses `tests/fixtures/pi5/pi5-valid.dtb`, an original minimal tree
generated by `tests/fixtures/pi5/generate.py`.
