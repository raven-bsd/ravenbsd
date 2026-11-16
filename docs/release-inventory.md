<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release image content inventory (REL7c)

Status: REL7c implementation. `scripts/release-inventory.sh` lists the initfs paths, sizes, modes
and CRC-32 values from a release disk image and checks the 0.1.0 release content. It is a
read-only host audit: it does not modify the image, build anything or claim a board result.

## Usage

```sh
sh scripts/release-inventory.sh build/x86_64/ravenbsd-x86_64.img
sh scripts/release-inventory.sh build/aarch64/ravenbsd-pi4.img
sh scripts/release-inventory.sh --initfs build/x86_64/initfs.img --kind uefi
```

The `--initfs` form is the host-test aid used by `tests/release/check-inventory.sh`; it skips the
disk-image extraction and runs the same content checks on a standalone initfs.

## Image detection and extraction

The image form reads partition 1 from the GPT (`sgdisk -p`) and inspects the FAT boot partition
with `mtools`:

* UEFI image: `raven/initfs.img` is copied out with `mcopy`.
* Pi 4 image: `kernel8.img` is copied out and the appended `RAVENFS1` archive is located by
  validating its header and requiring `start + total_size == kernel8 size`; the archive is then
  written to a temporary initfs file.

The initfs is listed with
`cargo run -q -p raven-initfs --features std --example lsinitfs`. Every entry is extracted to a
temporary tree for the ELF and path checks. The script never mounts the image, needs no root
privileges, and removes its temporary files on exit.

## Output

The output is key=value lines:

```
release-inventory: image=<path>
release-inventory: kind=<uefi|pi4>
release-inventory: initfs=<path> entries=<n>
release-inventory: path=<path> size=<n> mode=<octal> crc=<hex>
release-inventory: flag=<kind> <detail>
release-inventory: warn=<kind> <detail>
release-inventory: summary entries=<n> flags=<n> warnings=<n>
```

`path=` lines are the machine-readable inventory. `flag=` lines are fatal findings: the script
exits 1 when any flag is present. `warn=` lines are release-hygiene warnings and do not change the
exit code; they are reported so the release audit sees them.

## Critical paths

| Image | Required paths |
|---|---|
| both | `/bin/rsh`, `/bin/login`, `/bin/getty`, `/bin/ls`, `/bin/ping`, `/bin/brush`, `/etc/passwd`, `/etc/shadow` |
| UEFI release | `/sbin/virtio-net`, `/sbin/netd`, `/sbin/resolvd`, `/etc/services.d/netd.toml`, `/etc/services.d/resolvd.toml`, `/etc/drivers.toml` |
| Pi 4 release | `/sbin/genet`, `/sbin/devd`, `/sbin/platd`, `/sbin/netd`, `/etc/services.d/genet.toml`, `/etc/services.d/devd.toml`, `/etc/services.d/netd.toml`, `/etc/pi4.dtb`, `/etc/drivers.toml` |

The NIC/GENET manifests are also checked for their contract lines:

* UEFI: `netd.toml` declares `scheme:nic` and `scheme:net`; `resolvd.toml` declares
  `scheme:net`; `drivers.toml` names `virtio-net`.
* Pi 4: `genet.toml` declares `scheme:nic` and stays `enable = false` (devd is the only
  launcher); `devd.toml` uses `["devd", "dtb", "launch"]` with `platform-root`; `netd.toml`
  declares `scheme:nic`; `drivers.toml` names `brcm,bcm2711-genet-v5` and `/sbin/genet`.

## Fatal flags

* `missing-critical path=<path>` — a required path above is absent.
* `nic-contract path=<file> missing=<item>` — a NIC/GENET manifest lacks its scheme or launcher
  contract.
* `non-executable path=<path> mode=<octal>` — a file under `/bin` or `/sbin` has no execute bit.
* `not-stripped path=<path>` — an ELF has a symbol table that `file` does not report as stripped.
* `build-path path=<path> match=/target/` — an ELF embeds a `/target/` build path.
* `repo-path path=<path> match=<repo root>` — an ELF embeds the verifier's repository root.
* `duplicate-launcher name=<name> count=<n> paths=<a> <b>` — more than one executable under
  `/bin` or `/sbin` has the same basename, so one driver has more than one launcher path.

## Warnings

* `text-build-path` / `text-repo-path` — a non-ELF file (for example a shell-script comment)
  mentions `/target/` or the repository root. These are surfaced but not fatal: the ticket's
  binary-path check is about shipped ELF contents.
* `host-path path=<path>` — an ELF or text file contains `/home/`, `/media/` or `/Users/`.
  Cargo registry and cache paths commonly remain in panic-location strings even in stripped
  release binaries; the release audit decides whether to remap them.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | inventory produced, no fatal flags (warnings may be present) |
| 1 | at least one fatal flag |
| 2 | usage, missing image/initfs, missing host tool, or extraction/parse failure |

## Observed 0.1.0 inventories

**OBSERVED** in `tickets/kevin/reports/REL7c.md`:

* `build/x86_64/ravenbsd-x86_64.img` (UEFI): 194 entries, 1 fatal flag,
  22 warnings. The flag is `missing-critical path=/bin/ping`: the release stages `ping` at
  `/sbin/ping`, and `/sbin` is on the image's `PATH`, but the ticket's required `/bin/ping`
  entry is absent. The warnings are 21 `host-path` cargo-registry strings and one
  `text-build-path` in `/etc/brush-compat.sh` (`target/debug` in a comment).
* `build/aarch64/ravenbsd-pi4.img` (Pi 4): 80 entries, 0 fatal flags, 17 `host-path`
  cargo-registry warnings. All critical paths, including `/bin/ping` and the GENET files, are
  present; `genet.toml` is disabled and `devd.toml` carries the `dtb launch` / `platform-root`
  contract.

The Pi 4 image used for this inventory was built with
`RAVEN_RPI_FIRMWARE=<donor>/rpi-firmware/boot RAVEN_USER_RELEASE=1 cargo xtask image --arch aarch64
--platform pi4 --profile pi4-release`; the x86_64 image was the REL7b release build. The inventory
is host-side only and makes no board claim.

## Tests

```sh
sh -n scripts/release-inventory.sh
sh tests/release/check-inventory.sh
```

`tests/release/check-inventory.sh` builds two synthetic initfs images with `mkinitfs`: a complete
UEFI set that must report `flags=0`, and one missing `/bin/rsh` with a duplicate `/bin/ls`
`/sbin/ls` launcher and a non-executable `/bin/tool`, which must report those three flags. The test
does not build a disk image or run QEMU.

## Non-goals

* No board/link/DHCP/ping claim; a Pi 4 board log belongs to REL3/REL4.
* No image or product-file edits. The inventory reports findings; fixing a finding is a separate
  ticket.
* The Pi 4 initfs extraction relies on the REL5 flat `kernel8.img + initfs` layout. A future
  layout change must update `extract_pi4` and the REL7c test.
