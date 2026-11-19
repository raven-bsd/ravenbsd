<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-flash: safe removable-media image writer

Status: **host tool implemented and host-tested (K180).** TUI, list, dry-run and the privileged
helper revalidation are present. A real disposable SD/USB write is owner evidence; this page makes
no board or real-card claim.

The TUI is a first-party renderer over `crossterm`. `ratatui` was evaluated but its transitive
licence expressions (`cassowary` `MIT / Apache-2.0`, and optional `termwiz` crates such as
`terminfo`/`wezterm-bidi`) fail the mandatory provenance allow-list; no allow-list or donor entry
was edited to work around it. The four-screen renderer and its snapshots live in
`tools/raven-flash/src/tui.rs`.

`raven-flash` writes a **raw image** (ESP/FAT + UFS) to a removable USB/MMC/SD target. It is not a
formatter and not the UFS updater: K178 `raven-ufs` updates files inside an existing UFS partition
without reflashing. No kernel, loader, boot or product source is changed.

## 1. Safety model

- The image is a raw `.img`; the tool shows path, size, SHA-256 and, when an adjacent
  `manifest.txt`/`build-id.txt` exists, the build id.
- Targets come from `/sys/block` and `/proc/mounts`. Only removable USB/MMC/SD disks are safe.
- Refused: the root/boot disk, fixed/internal disks, loop/LVM/md/virtual devices, disks with
  sysfs holders, non-removable disks, symlink device paths, images that do not fit, and a target
  that contains the image file.
- Mounted partitions are shown and require an explicit confirmation before `umount`; if any
  unmount fails the write is refused.
- The TUI runs as the user. The write elevates through `sudo`/`pkexec` to
  `raven-flash --helper-write`, which **revalidates** the image and target from `/sys`/`/proc/mounts`
  and does not trust the UI selection. A typed device-name confirmation is required.
- The write is a raw `write`, `fsync`, then a readback SHA-256 comparison (default on). A partial
  or failed write is reported; no cancel path silently claims success.

## 2. TUI (four screens)

1. **Select image** — raw `.img` files under `build/` and the current directory.
2. **Select target** — removable devices with model, serial, size, transport and mount points;
   refused devices stay visible with the reason.
3. **Confirm** — image/target summary, mounted partitions that will be unmounted, and a typed
   device-name confirmation.
4. **Write/verify** — bytes/progress, image and target SHA-256, verification result.

Run it with `cargo xtask`-free direct invocation:

```sh
scripts/raven-flash.sh
# or
cargo run -q -p raven-flash
```

A non-TTY invocation requires an explicit path.

## 3. Explicit non-interactive paths

```sh
cargo run -q -p raven-flash -- --list
cargo run -q -p raven-flash -- --dry-run --image build/aarch64/ravenbsd-pi4-sd.img
cargo run -q -p raven-flash -- --yes --image IMG --device /dev/sdX
```

`--list` prints every block device as `SAFE` or `REFUSED <reason>`. `--dry-run` hashes the image,
applies the filter and prints candidates; with `--device` it validates the pair and reports the
unmount it would request, without writing. `--yes` is the explicit write path; it still refuses
mounted partitions and elevates through the helper.

The helper can be called directly by an owner/elevation wrapper:

```sh
cargo run -q -p raven-flash -- --helper-write --image IMG --device /dev/sdX --confirm sdX
```

## 4. Host evidence

- `cargo nextest run -p raven-flash` — 17/17 tests: the device filter (root, fixed, mounted, swap,
  LVM/md/loop, holders, too-small, target==image, symlink), image size/hash/build-id, write/
  readback verification, CLI parsing and TUI snapshots for the four screens.
- `--list` on this host returns 0 and shows fixed/loop/root disks refused; no removable target is
  attached.
- `--dry-run --image build/aarch64/ravenbsd-pi4-sd.img` returns 0, hashes the 500,187,648-byte
  image and reports no target selected.

The synthetic fixtures are built in `tools/raven-flash/src/device_ut.rs`; a static fixture tree
lives under `tests/flash/fixtures/` for review. No real card is written by CI.

## 5. Owner evidence (not a CI claim)

Write a disposable card, then capture:

- the TUI/selected image and target summary (model, serial, size);
- the image and target SHA-256 after `fsync`/readback;
- `dmesg`/`lsblk` before and after, and the successful `pi4-install.sh --check` or board boot.

Until that transcript exists, the real-card write is **UNVERIFIED**. UFS in-place updates are
K178, not this tool.

## 6. DR-52

First-party host tooling only: `crossterm` (MIT), `sha2` and `anyhow` are permissive dependencies;
no C, no Electron, no web runtime, no donor code copied. The device filter, the four-screen
renderer and the revalidating helper are original RavenBSD code.
