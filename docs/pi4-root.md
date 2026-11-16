<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 EMMC2 root path

J5 wires the EMMC2 `disk:` scheme into blockd and the read-only UFS root through fsd-ufs and vfsd.
The board path is **UNVERIFIED**; the qtest uses the host-backed EMMC2 test mode and proves the
userspace storage stack, not the BCM2711 MMIO/IRQ/SDMA path.

## Test stack

`cargo xtask qtest --case pi4-root --arch aarch64` builds `tests/fixtures/pi4-root/build.sh` and
boots the standalone `pi4-root-test` profile:

1. `emmc2 test` loads `initfs:/etc/pi4-root.img` and registers the `disk:` scheme.
2. `blockd` opens `disk:0`, scans the GPT and serves `block:0`.
3. `fsd-ufs` mounts partition 1 read-only and serves `ufs:`.
4. `vfsd` mounts the tree and the one-shot probe reads `file:/ufs/...`.
5. The probe checks the known files, directory listing, symlink, stat, error cases and umount;
   then svcd settles and init exits 0.

Serial markers:

```
emmc2: backend=memory image=initfs:/etc/pi4-root.img sectors=32835
emmc2: registered disk
emmc2: serving
blockd: gpt ok partitions=1
blockd: part 1 ufs-root 34..32801
fsd-ufs: mounted block:0 partition 1 as ufs: (16777216 bytes)
svcd: root mounted at /root
fsd-ufs: probe mount ok
fsd-ufs: probe ten.txt=0123456789
fsd-ufs: probe ok
init exited (0)
```

The K54a vfsd proxy fix is required for the `file:` read; `vfs-aarch64` and the literal
`fsd-ufs --arch aarch64` case remain the regression checks.

## Production profile

`profiles/pi4.d/10-emmc2-blockd.toml` stages the EMMC2 and blockd manifests with
`enable = false`; the J3 fragment deliberately does not claim a board boot. Enabling the storage
stack in the real `pi4` image still needs:

* the supervisor/`platd` handoff of the EMMC2 MMIO window, DMA buffer and GIC interrupt;
* the core service set (`logd`, `initfsd`, `ramfsd`, `vfsd`, ...) in the Pi image;
* a root-mount/switch-root step that replaces the initfs root with `/root`;
* a Pi 4 board log for the first card initialisation and UFS read.

Until then the real-image path remains UNVERIFIED and the test profile is the only executed
evidence.
