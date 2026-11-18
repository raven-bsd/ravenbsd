<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 SD image: FAT boot + UFS2 root

Date: 2026-11-18. Ticket: SD1. Host-only image lane; board evidence belongs to SD3.

## Layout

`build/aarch64/ravenbsd-pi4-sd.img` is one deterministic GPT disk:

| Partition | Type GUID | LBA | Size | Contents |
|---|---|---|---|---|
| BOOT | EFI System `28732ac1-1ff8-d211-ba4b-00a0c93ec93b` | 2048..452607 | 220 MiB FAT32 | `config.txt`, `cmdline.txt`, flat `kernel8.img`, `bcm2711-rpi-4-b.dtb`, firmware and `overlays/disable-bt.dtbo` |
| ROOT | FreeBSD UFS `516e7cb6-6ecf-11d6-8ff8-00022d09712b` | 452608..976895 | 256 MiB UFS2 | release userland, services, `/etc`, `/root`, shell/network/disk tools |

The protective MBR, primary/backup GPT headers and entry arrays are written explicitly;
the FAT partition is built by the deterministic `FatImageBuilder`, then re-created with
`cmdline.txt` so the file timestamps stay fixed by `SOURCE_DATE_EPOCH`. The UFS2 root is
formatted by `ufs-mkfs` with the same epoch and populated from the resolved `pi4-rootfs`
profile plus `/root/.keep`.

The FAT `kernel8.img` is the flattened Pi 4 kernel with the release initfs appended, exactly
as the normal Pi image; the UFS root is the installable root for SD2/SD3. No board result is
claimed by this host-only lane.

## Build

```sh
RAVEN_RPI_FIRMWARE=/path/to/rpi-firmware/boot \
SOURCE_DATE_EPOCH=1700000000 \
    cargo xtask image --arch aarch64 --platform pi4 --profile pi4-rootfs
```

The profile is `profiles/pi4-rootfs.toml`; its fragments are the release userland/service set
(`profiles/pi4-rootfs.d/*`), and `profiles/pi4-rootfs/rootfs.toml` fixes the UFS size and adds
`/root/.keep`. The rootfs staging tree is `build/aarch64/rootfs-staging`.

Determinism: two clean builds with the same `SOURCE_DATE_EPOCH` produced the same image hash
`da9bf2a0d856b3cf0a42fee0199290f3cecd980dd38b7ff95b9420291d229e1f` (500,187,648 bytes,
976,929 sectors).

## Check and flash

```sh
# Host checks: GPT CRCs/partitions, FAT manifest and cmdline, UFS fsck, root tree
sh scripts/pi4-install.sh --check

# The Pi release checker accepts the same GPT image
sh scripts/pi4-release.sh --check -i build/aarch64/ravenbsd-pi4-sd.img

# Owner-only write; refuses non-root, non-block, mounted and root disks; size + confirmation
sudo sh scripts/pi4-install.sh --flash /dev/sdX
```

`pi4-install.sh --check` prints the GPT layout, each FAT file/size/check, the UFS fsck result
and every required root path. `--flash` checks `id -u`, block-device type, `/proc/mounts` for the
device and all children, the root disk/partition, the target size and an explicit `YES` (unless
`--yes`). It never writes without those checks.

## SD3 board steps

1. Flash the image to a Pi 4 SD card with `pi4-install.sh --flash` (or `dd` on a non-root device).
2. Connect the UART at 115200 8N1 and capture the release markers: kernel banner, `login:`.
3. Log in as root (development password), then run `ls /bin`, `uname -a` and `ping 10.0.2.2`
   with 0% packet loss; exit cleanly.
4. Record the board transcript in the SD3 report. This host-only note does not claim a board run.

## Donor reconciliation (DR-52/DR-57)

| Topic | Illumos | BSD | Linux | Decision |
|---|---|---|---|---|
| GPT + FAT + UFS install layout | `installboot`/`fdisk` paths are not in the sparse illumos checkout; no code copied | NetBSD `usr.sbin/makefs/ffs/mkfs.c` (revision `36eddebcc36cb0775aa2962005b9c05419a6ff01`) underpins the existing `ufs-core` port; FreeBSD `sbin/newfs`/`sbin/gpart` (revision `133344405f532bd54ff3629a77d4c4e062f01fdb`) provide the GPT/UFS install behaviour | `mkfs.fat`, `parted`/`util-linux` are outside the sparse Linux checkout; outcome only, no code copied | Reuse the existing `build_ufs_root`/`build_root_disk` and the deterministic FAT builder; assemble GPT in first-party Rust |
| Deterministic image timestamps | n/a | NetBSD makefs uses a fixed timestamp; existing `ufs-core` port follows it | Reproducible-build epoch practice | `SOURCE_DATE_EPOCH` controls FAT and UFS metadata; two builds compare byte-for-byte |
| Host-only verification | n/a | fsck/UFS tools; no board claim | n/a | `pi4-install.sh --check` and `pi4-release.sh --check` prove GPT/FAT/UFS; SD3 owns board evidence |

No CDDL/GPL code was copied. The UFS formatter is the existing BSD-3-Clause `ufs-core` port with
its recorded NetBSD provenance; this ticket adds first-party GPT/FAT assembly and shell checks.
