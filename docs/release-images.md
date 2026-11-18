<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release image reproducibility (IMG1)

Status: **OBSERVED at product commit `6d1b1bde`.** The x86_64 `release` image and the Pi 4
FAT+UFS `pi4-rootfs` SD image are byte-identical across two builds at the same
`SOURCE_DATE_EPOCH`. Host-only; no board or QEMU run is claimed.

## 1. Inputs

| Item | Value |
|---|---|
| Product commit | `6d1b1bde` (`origin/master`, clean tree at build time) |
| Branch | `mel/IMG1` (docs-only; the build used `6d1b1bde`) |
| `SOURCE_DATE_EPOCH` | `1700000000` = 2023-11-14T22:13:20Z |
| Raspberry Pi firmware | `/home/bo/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot` |
| Pi 4 boot config | `xtask/src/pi.rs` (1024x768x32, DMT 16, `enable_uart=1`, `uart_2ndstage=1`) |
| Pi 4 DTB fixture | `tests/fixtures/pi4-platd/pi4-platd.dtb.hex` via `tests/fixtures/pi4-release/build.sh` |
| x86_64 boot | UEFI loader + `raven/kernel.elf` + `raven/initfs.img` in the ESP |
| Pi 4 boot | firmware + `kernel8.img` + `config.txt` + `cmdline.txt` + DTB in FAT; UFS2 root partition |
| Toolchain | pinned repo toolchain (`rust-toolchain.toml`) |

"Clean fixture state" means `build/fixtures` is removed before **each** build; the image builder
regenerates the Pi 4 DTB fixture from the committed hex. The kernel target directory is cleaned
once before each pair so a cached artifact from an older commit cannot be reused. The working tree
must be clean (the release image builder enforces this).

## 2. x86_64 release image

Commands (the second build repeats the first after `rm -rf build/fixtures`):

```sh
rm -rf build/fixtures
cargo clean -p raven-kernel --target-dir build/cargo/kernel-x86_64
SOURCE_DATE_EPOCH=1700000000 cargo xtask image --arch x86_64 --profile release \
  --output /tmp/opencode/img1-r1/x86-a.img

rm -rf build/fixtures
SOURCE_DATE_EPOCH=1700000000 cargo xtask image --arch x86_64 --profile release \
  --output /tmp/opencode/img1-r1/x86-b.img
```

| Build | Size | SHA-256 |
|---|---:|---|
| A | 335,544,320 | `66703e8fc733338506b45542345e794c917e85358fc46ba27012c9eb18db699e` |
| B | 335,544,320 | `66703e8fc733338506b45542345e794c917e85358fc46ba27012c9eb18db699e` |

`cmp` reports byte-identical; both builds removed `build/fixtures` first.

## 3. Pi 4 FAT+UFS SD image (`pi4-rootfs`)

Commands (the second build repeats the first after `rm -rf build/fixtures`):

```sh
rm -rf build/fixtures
cargo clean -p raven-kernel --target-dir build/cargo/kernel-aarch64-pi4
RAVEN_RPI_FIRMWARE=/home/bo/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot \
SOURCE_DATE_EPOCH=1700000000 \
  cargo xtask image --arch aarch64 --platform pi4 --profile pi4-rootfs \
  --output /tmp/opencode/img1-r1/pi4-a.img

rm -rf build/fixtures
RAVEN_RPI_FIRMWARE=/home/bo/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot \
SOURCE_DATE_EPOCH=1700000000 \
  cargo xtask image --arch aarch64 --platform pi4 --profile pi4-rootfs \
  --output /tmp/opencode/img1-r1/pi4-b.img
```

| Build | Size | SHA-256 |
|---|---:|---|
| A | 500,187,648 | `7ea6d8b230e02ebae35078df778ef8d708fcf2b3bae1dccd50f6cd6681ef0208` |
| B | 500,187,648 | `7ea6d8b230e02ebae35078df778ef8d708fcf2b3bae1dccd50f6cd6681ef0208` |

`cmp` reports byte-identical. The GPT (FAT boot + UFS root), FAT firmware/config/kernel/DTB and
UFS2 root tree are all deterministic under the epoch. The FAT directory timestamp is
2023-11-14 22:13 (the epoch).

## 4. Host checks

### Pi 4

```sh
sh scripts/pi4-install.sh --check -i /tmp/opencode/img1-r1/pi4-a.img
```

Exit 0:

```text
pi4-install: GPT ok fat=2048+450560 ufs=452608+524288 total=976929
pi4-install: FAT config.txt ... config ok
pi4-install: FAT cmdline.txt ... cmdline ok
pi4-install: FAT kernel8.img ... kernel ok
pi4-install: FAT bcm2711-rpi-4-b.dtb ... fdt ok
pi4-install: image check PASS sha256=7ea6d8b230e02ebae35078df778ef8d708fcf2b3bae1dccd50f6cd6681ef0208
pi4-install: board evidence remains UNVERIFIED (SD3)
```

The release-content inventory (`sh scripts/release-inventory.sh /tmp/opencode/img1-r1/pi4-a.img`)
reports `entries=89 flags=0 warnings=17` and exits 0.

### x86_64

The x86 image has no Pi 4 manifest; its FAT boot manifest is the UEFI ESP content at the 1 MiB
offset:

```sh
mdir  -i /tmp/opencode/img1-r1/x86-a.img@@1M ::/
mtype -i /tmp/opencode/img1-r1/x86-a.img@@1M ::/EFI/BOOT/BOOTX64.EFI
mtype -i /tmp/opencode/img1-r1/x86-a.img@@1M ::/raven/kernel.elf
mtype -i /tmp/opencode/img1-r1/x86-a.img@@1M ::/raven/initfs.img
mtype -i /tmp/opencode/img1-r1/x86-a.img@@1M ::/raven/boot.toml
```

All exit 0; `mdir` shows `EFI` and `RAVEN` directories. This is the x86 image manifest check.

The optional release-content inventory (`sh scripts/release-inventory.sh .../x86-a.img`) reports
`entries=211 flags=1 warnings=22`; the flag is the pre-existing REL7c finding
`missing-critical path=/bin/ping` (the release stages ping at `/sbin/ping`). That is a release
content/path issue outside IMG1's reproducibility Owns; it is recorded here and handed to the
release/profile owner (REL7c/Kevin). It does **not** indicate a non-deterministic build.

## 5. Result

| Gate | Result |
|---|---|
| x86_64 release, two clean-fixture builds, same epoch | **byte-identical** |
| Pi 4 `pi4-rootfs` SD, two clean-fixture builds, same epoch | **byte-identical** |
| `pi4-install.sh --check` | **PASS** |
| Pi 4 release inventory | **PASS** (0 flags, 17 host-path warnings) |
| x86 FAT manifest (`mdir`/`mtype`) | **PASS** |
| x86 release inventory | **1 pre-existing `/bin/ping` flag**, outside IMG1 |
| `make check-fast` | **PASSED** |

No board, QEMU, reflash or donor code was used. The current release-evidence packet can pin the
two image hashes to `6d1b1bde` with `SOURCE_DATE_EPOCH=1700000000`; the `/bin/ping` content flag
remains a separate release-gate item.

## 6. Donor reconciliation (DR-57 / DR-52)

IMG1 builds and compares existing artifacts; it changes no algorithm, format or donor-derived code.
No C and no donor code was copied. Determinism comes from the first-party FAT/GPT/UFS writers and
`SOURCE_DATE_EPOCH`; the Pi 4 DTB is the committed fixture hex. DR-57/DR-52 are not applicable to
this evidence-only ticket.
