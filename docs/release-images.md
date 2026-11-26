<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release image reproducibility (IMG1)

Status: **OBSERVED at product commit `fd394ff3`.** The x86_64 `release` image and the Pi 4
`pi4-rootfs` SD image both build byte-identically in an A/B pair under
`SOURCE_DATE_EPOCH=1700000000`; the x86 pair carries the fresh K131 loader. Host-only; no board
or QEMU run is claimed.

## 1. Inputs

| Item | Value |
|---|---|
| Product commit | `fd394ff3` (`origin/master`, clean tree at build time) |
| Branch | `mel/K132` (docs-only; the build used `fd394ff3`) |
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

Commands (each build removes `build/fixtures`; the kernel target directory is cleaned once
before the pair, and a release build purges its own `brush`/`uutils` userspace targets):

```sh
cargo clean -p raven-kernel --target-dir build/cargo/kernel-x86_64
rm -rf build/fixtures
SOURCE_DATE_EPOCH=1700000000 cargo xtask image --arch x86_64 --profile release \
  --output /tmp/opencode/k132/x86-a.img

rm -rf build/fixtures
SOURCE_DATE_EPOCH=1700000000 cargo xtask image --arch x86_64 --profile release \
  --output /tmp/opencode/k132/x86-b.img
```

| Build | Size | SHA-256 |
|---|---:|---|
| A | 335,544,320 | `08c7a841cf8ff6a70fcf34d9d9c0d5d8d50c06789fb6d97e438b64ecbbb799b7` |
| B | 335,544,320 | `08c7a841cf8ff6a70fcf34d9d9c0d5d8d50c06789fb6d97e438b64ecbbb799b7` |

`cmp` reports byte-identical; both builds removed `build/fixtures` first, and the kernel target
directory was cleaned once before the pair.

### K131 loader freshness

The release build purges `build/<arch>/{libc.so.1,ld-elf.so.1}` and
`build/<arch>/loader-manifest.toml`, so `libc-link` always produces the loader from the current
sources. It then gates the linked loader and writes `build/x86_64/loader-manifest.toml`:

```text
arch = "x86_64"
revision = "fd394ff3"
image_base = "0x1000"
interpreter = "/libexec/ld-elf.so.1"
size = 107128
crc32 = "08c0f51a"
sha256 = "7bc53ceceaf31bdc878d31461075ccd3f0a1029028003af1a82f30d3cba46e09"
```

Extracting the image's initfs (`mtype -i x86-a.img@@1M ::/raven/initfs.img`) and listing it reports
the same entry:

```text
/libexec/ld-elf.so.1 107128 0755 08c0f51a
```

The pre-fix proof is preserved in the K131 report: on `4d3e1576` a 14-byte `STALE-NOT-ELF` file
was packed unchanged as `/libexec/ld-elf.so.1` (size 14, CRC-32 `8cfdbaec`); the fixed build
replaces and rejects that shape.

## 3. Pi 4 FAT+UFS SD image (`pi4-rootfs`)

Rebuilt at `fd394ff3`, the same product commit as the x86 image. K131's loader freshness applies
to the shared `initfs::build` path, so this build also purges and verifies the dynamic loader.

Commands (the second build repeats the first after `rm -rf build/fixtures`):

```sh
cargo clean -p raven-kernel --target-dir build/cargo/kernel-aarch64-pi4
rm -rf build/fixtures
RAVEN_RPI_FIRMWARE=/home/bo/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot \
SOURCE_DATE_EPOCH=1700000000 \
  cargo xtask image --arch aarch64 --platform pi4 --profile pi4-rootfs \
  --output /tmp/opencode/k132/pi4-a.img

rm -rf build/fixtures
RAVEN_RPI_FIRMWARE=/home/bo/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot \
SOURCE_DATE_EPOCH=1700000000 \
  cargo xtask image --arch aarch64 --platform pi4 --profile pi4-rootfs \
  --output /tmp/opencode/k132/pi4-b.img
```

| Build | Size | SHA-256 |
|---|---:|---|
| A | 500,187,648 | `7d84cad0afdb34479141de8309c02bd6e91d46cf9c6a897b9f463bf81c692b66` |
| B | 500,187,648 | `7d84cad0afdb34479141de8309c02bd6e91d46cf9c6a897b9f463bf81c692b66` |

`cmp` reports byte-identical. The GPT (FAT boot + UFS root), FAT firmware/config/kernel/DTB and
UFS2 root tree are all deterministic under the epoch. The FAT directory timestamp is
2023-11-14 22:13 (the epoch).

## 4. Host checks

### Pi 4

```sh
sh scripts/pi4-install.sh --check -i /tmp/opencode/k132/pi4-a.img
```

Exit 0:

```text
pi4-install: GPT ok fat=2048+450560 ufs=452608+524288 total=976929
pi4-install: FAT config.txt ... config ok
pi4-install: FAT cmdline.txt ... cmdline ok
pi4-install: FAT kernel8.img ... kernel ok
pi4-install: FAT bcm2711-rpi-4-b.dtb ... fdt ok
pi4-install: image check PASS sha256=7d84cad0afdb34479141de8309c02bd6e91d46cf9c6a897b9f463bf81c692b66
pi4-install: board evidence remains UNVERIFIED (SD3)
```

The release-content inventory (`sh scripts/release-inventory.sh /tmp/opencode/k132/pi4-a.img`)
reports `entries=89 flags=0 warnings=17` and exits 0.

### x86_64

The x86 image has no Pi 4 manifest; its FAT boot manifest is the UEFI ESP content at the 1 MiB
offset:

```sh
mdir  -i /tmp/opencode/k132/x86-a.img@@1M ::/
mtype -i /tmp/opencode/k132/x86-a.img@@1M ::/EFI/BOOT/BOOTX64.EFI
mtype -i /tmp/opencode/k132/x86-a.img@@1M ::/raven/kernel.elf
mtype -i /tmp/opencode/k132/x86-a.img@@1M ::/raven/initfs.img
mtype -i /tmp/opencode/k132/x86-a.img@@1M ::/raven/boot.toml
```

All exit 0; `mdir` shows `EFI` and `RAVEN` directories. This is the x86 image manifest check.

The optional release-content inventory (`sh scripts/release-inventory.sh .../x86-a.img`) reports
`entries=211 flags=1 warnings=22`; the flag is the pre-existing REL7c finding
`missing-critical path=/bin/ping` (the release stages ping at `/sbin/ping`). That is a release
content/path issue outside IMG1's reproducibility scope; it is recorded here and handed to the
release/profile owner (REL7c/Kevin). It does **not** indicate a non-deterministic build.

## 5. Kernel and loader profile (K170)

"Release image" means release **kernel, loader and user programs**. The `release` and
`pi4-release` profiles build the kernel and loader with the release profile; test profiles keep the
debug build. A demo/test profile can request release kernel and loader explicitly:

```toml
[image]
release = true
```

The image builder verifies the build-profile stamp beside the artifact it embeds
(`build/<arch>/kernel.elf.profile` and `loader.efi.profile`) and rejects a release image whose
kernel or loader stamp says `debug`. Debug artifacts use `kernel-debug.elf`/`loader-debug.efi`, so a
debug image build does not overwrite a release `kernel.elf`/`loader.efi`. The release kernel also
keeps the clean-HEAD build-id check. See `docs/build-image.md` for the full path and stamp table.

The before/after evidence for this change is in `tickets/kevin/reports/K170.md`.

## 6. Result

| Gate | Result |
|---|---|
| x86_64 release A/B, clean fixtures + kernel target, same epoch | **byte-identical** (`08c7a841…`) |
| x86 loader first `PT_LOAD` base | **0x1000** |
| x86 loader manifest interpreter/hash | **`/libexec/ld-elf.so.1`; size 107128, CRC-32 `08c0f51a`, SHA-256 `7bc53cec…`** |
| x86 image initfs loader entry | **matches the fresh loader** (same size/CRC-32) |
| Stale 14-byte loader | **replaced and rejected** (pre-fix packed it unchanged) |
| Pi 4 `pi4-rootfs` SD A/B, clean fixtures, same epoch | **byte-identical** (`7d84cad0…`) |
| x86 FAT manifest (`mdir`/`mtype`) | **PASS** |
| x86 release inventory | **1 pre-existing `/bin/ping` flag**, outside IMG1 |
| `make check-fast` | **PASSED** |

No board, QEMU, reflash or donor code was used. Both A/B pairs are pinned to `fd394ff3` with
`SOURCE_DATE_EPOCH=1700000000`; the `/bin/ping` content flag remains a separate release-gate item.

## 7. Donor reconciliation (DR-52)

K131 changes first-party build orchestration (`xtask`): it purges stale dynamic outputs, rebuilds
them, and gates the loader contract. It copies no donor code and implements no donor algorithm.

| Topic | Illumos | BSD | Linux | RavenBSD decision |
|---|---|---|---|---|
| Loader image base | Not consulted; the loader is RavenBSD's `raven-libc-rtld`. | Not consulted. | Not consulted. | Keep the K7 `--image-base=0x1000` contract. |
| Build cache freshness | Not applicable. | Not applicable. | Not applicable. | A release profile purges the dynamic pair and requires a matching manifest; no cached-artifact shortcut. |
| Manifest hash | Not applicable. | Not applicable. | Not applicable. | Record revision, base, interpreter, size, CRC-32 and SHA-256 beside the loader; compare before reuse. |

