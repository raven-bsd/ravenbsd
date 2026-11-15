<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 5 FAT manifest fixture (R8)

`build.sh` creates `build/fixtures/pi5-image/pi5-boot.img`, a **synthetic** 16 MiB raw image with
a FAT volume at the 1 MiB partition offset, and runs:

```
scripts/pi5-serial.sh --check -i build/fixtures/pi5-image/pi5-boot.img
```

The files are synthetic except for the DTB, which is the original minimal
`tests/fixtures/pi5/pi5-valid.dtb` test tree. `kernel_2712.img` is a non-zero placeholder, not a
bootable kernel; `start4.elf` and `fixup4.dat` are size placeholders. The fixture is a host-side
check of the manifest parser, not a Pi 5 image and not hardware evidence.

`manifest.txt` is the real staging manifest used by `scripts/pi5-serial.sh`; its fields are
`path`, `min_bytes` and `check` (`config`, `kernel`, `fdt` or `none`). The real firmware and DTB
come from the read-only `ravenbsd-resources/rpi-firmware/boot` checkout and are never committed.
