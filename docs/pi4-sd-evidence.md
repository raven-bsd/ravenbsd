<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 SD install and serial persistence evidence (SD3)

- Date: 2026-11-18
- Product base: `4af39574` (origin/master; SD1/SD2 merged)
- Status: **HOST CHECKS PASS; BOARD EVIDENCE BLOCKED.** No board/serial hardware is attached to
  this host and the owner is away. No board transcript, persistence proof or Pi 4 PASS is claimed.
- Scope: the SD1 UFS/SD image, the SD2 read-write root, and the SD3 serial/persistence gate. The
  E-Ink/HDMI panel is a separate gate and is not part of this page.

## Image under test

| Item | Value |
|---|---|
| Commit | `4af39574` |
| Build | `RAVEN_RPI_FIRMWARE=/home/bo/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot SOURCE_DATE_EPOCH=1700000000 cargo xtask image --arch aarch64 --platform pi4 --profile pi4-rootfs` |
| Image | `build/aarch64/ravenbsd-pi4-sd.img` |
| Size | 500,187,648 bytes |
| SHA-256 | `5aca161509d40f6a8fbf2cad6a5ab338e00c94c1907518818778c6f5775aafa6` |

SD1's earlier image at its own commit was `da9bf2a0d856b3cf0a42fee0199290f3cecd980dd38b7ff95b9420291d229e1f`;
the current tree includes later merges, so the hash above is the one to use for this run.

## Host-side checks (OBSERVED, no serial/board access)

### `sh scripts/pi4-install.sh --check` — exit 0

```
pi4-install: GPT ok fat=2048+450560 ufs=452608+524288 total=976929
pi4-install: FAT ... ok
/tmp/pi4-install.*/root.ufs: clean
pi4-install: UFS path /init ok
pi4-install: UFS path /bin/brush ok
pi4-install: UFS path /bin/ls ok
pi4-install: UFS path /bin/uname ok
pi4-install: UFS path /bin/ping ok
...
pi4-install: UFS path /root/.keep ok
pi4-install: image check PASS sha256=5aca161509d40f6a8fbf2cad6a5ab338e00c94c1907518818778c6f5775aafa6
pi4-install: board evidence remains UNVERIFIED (SD3)
```

### `sh scripts/pi4-release.sh --check -i build/aarch64/ravenbsd-pi4-sd.img` — exit 0

```
pi4-install: ... (same GPT/FAT/UFS checks)
pi4-install: image check PASS sha256=5aca161509d40f6a8fbf2cad6a5ab338e00c94c1907518818778c6f5775aafa6
pi4-install: board evidence remains UNVERIFIED (SD3)
pi4-release: SD FAT+UFS layout PASS
pi4-release: image check PASS (board gates remain UNVERIFIED)
```

These checks confirm the image layout, FAT, UFS fsck and the required root tree. They do **not**
prove a board boot, a read-write mount or persistence.

## Exact owner action (blocked on hardware)

The owner has the Pi 4 serial hardware, SD card and adapter; the host currently has **no**
`/dev/ttyUSB*`/`/dev/ttyACM*` and no SD device selected. Do not write a disk automatically.

1. **Identify the SD device (owner, read-only first):**
   ```sh
   lsblk -o NAME,SIZE,TYPE,MOUNTPOINTS
   ```
   Choose the SD card block device (example `/dev/sdX`); it must not be mounted and must not be
   the disk containing `/`. The installer re-checks root/mount/size and refuses unsafe targets.

2. **Flash the SD image (owner runs/approves; root required):**
   ```sh
   sudo sh scripts/pi4-install.sh --flash /dev/sdX --yes \
     -i build/aarch64/ravenbsd-pi4-sd.img
   ```
   Without `--yes` the script prompts `Type YES to continue`. It refuses a mounted device, the
   root disk, a non-block device or a target smaller than the image.

3. **Wire and boot:** insert the SD in the Pi 4, connect the UART (GPIO14 TX, GPIO15 RX, GND;
   115200 8N1), power on. Default adapter `/dev/ttyUSB0`; override with `-d`.

4. **Capture the first boot** (foreground; the capture script uses the SD image for the FAT
   manifest check and writes a timestamped log):
   ```sh
   sh scripts/pi4-serial.sh -d /dev/ttyUSB0 -b 115200 \
     -i build/aarch64/ravenbsd-pi4-sd.img -l build/pi4/sd3-serial.log
   ```
   `scripts/pi4-release.sh --device /dev/ttyUSB0 --log build/pi4/sd3-serial.log \
     -i build/aarch64/ravenbsd-pi4-sd.img --no-check` is the equivalent capture path; its
   generic marker set includes `ls-rsh rsh`, which is a FAT-release-image marker and is **not**
   expected on the SD rootfs (the SD root tree has `brush`, `ls`, `uname`, `ping` and no rsh).
   Use the SD3 checklist below, not that single marker, for the SD gate.

5. **At the login prompt** (`root` / `raven`), capture:
   ```text
   login: root
   Password: raven
   raven# ls /bin
   raven# df
   raven# mount
   raven# echo sd3-persist > /root/sd3-persist.txt
   raven# sync
   raven# cat /root/sd3-persist.txt
   raven# route            # or: ip route; record the default gateway
   raven# ping <gateway>
   raven# reboot
   ```

6. **After the reboot/login**, prove persistence:
   ```text
   login: root
   Password: raven
   raven# cat /root/sd3-persist.txt     # must print: sd3-persist
   raven# ls /root
   raven# df /root
   raven# exit 0                         # clean init exit
   ```

7. **Record** the exact log path, image SHA-256, SD device, UART adapter, board revision and the
   first failure if any. Keep the first failing log; do not overwrite it with a later run.

## SD3 transcript checklist

The real board log must contain:

- [ ] firmware/kernel banner (`RavenBSD kernel 0.1 (aarch64)`) and the Pi 4 DTB path
- [ ] `fsd-ufs: mounted block:0 partition 1 as ufs:` with `mode=rw`
- [ ] `svcd: root mounted at /root`
- [ ] `login:` and a successful `root`/`raven` login
- [ ] brush prompt `raven#` and `ls /bin` (brush, ls, uname, ping)
- [ ] `df`/`mount` showing the UFS root mounted read-write at `/root`
- [ ] write under `/root`, reboot, and `cat /root/sd3-persist.txt` still prints the value
- [ ] `ping <gateway>` reply (and the gateway address recorded)
- [ ] clean `init exited (0)`, with no panic/EXCEPTION/timeout

No fixture or host result substitutes for the owner transcript.

## Blocker and owner action

**BLOCKED on the board/serial hardware:** no `/dev/ttyUSB*`/`/dev/ttyACM*` exists on this host,
the SD card is not attached, and the owner is away. The host checks pass, but the acceptance
requires the real captured log. The owner must run steps 1-6 above (or attach the SD/UART and
authorize the capture) and provide the log path; then this page can be updated with the exact
transcript and persistence proof.

Panel/HDMI status is recorded separately; SD3 is the UFS/SD gate only.

## DR-52 / DR-57 donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| SD image/install/host checks | no equivalent; storage design-only | FreeBSD GPT/UFS tooling (BSD-2) is a behaviour reference; no code copied | `sfdisk`/UFS outcomes; read-only | First-party `xtask`/`scripts/pi4-install.sh` and `pi4-release.sh`; no donor code |
| Serial capture/persistence proof | no equivalent | FreeBSD `release`/UFS behaviour reference | serial console/`dd` outcomes; read-only | First-party shell capture; owner transcript only |
| Board evidence | no board in this ticket | no permissive board log copied | no borrowed transcript | **Blocked:** owner hardware/action; no fake log |

No donor code was copied, translated or closely paraphrased. The loop is steal-idea →
implement → test → repeat (DR-57) with the permissive donors as references only.
