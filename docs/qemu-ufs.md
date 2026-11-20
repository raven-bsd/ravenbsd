<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU UFS root preflight (K176)

A QEMU image whose `/` is a real UFS2 partition staged from the release rootfs payload, with the
initfs as bootstrap only. **OBSERVED** on image `ebfce46b…`, root disk `af2f9e63…` (K176); K193
adds the full interactive disk-tool demo and the `ufs-full` qtest.

## Image layout

* ESP: UEFI loader + release kernel + `qemu-ufs-boot` initfs (scheme servers, K179 virtio-blk +
  virtio-net driver stack, `blockd`/`fsd-ufs`/`vfsd`, the console and the pivot marker).
* Root disk: a 256 MiB GPT/UFS2 disk built from the `qemu-ufs` payload profile (the release
  userland: getty/login, brush, uutils, netd/resolvd/ping), attached as `virtio-blk-pci`.
* `cargo xtask image --profile qemu-ufs --output <esp.img>` emits the ESP image and
  `build/x86_64/qemu-ufs-root.img`.

## Boot and pivot

1. The bootstrap plan starts `logd pcid devd initfsd nulld ramfsd randd zerod blockd fsd-ufs vfsd`.
   devd (K179) launches both virtio functions; `blockd` reads the GPT and `fsd-ufs` reports
   `mounted block:0 partition 1 as ufs: (268435456 bytes) mode=rw`.
2. `svcd` reads `/etc/raven/root-mode` (`pivot`), probes `ufs:/etc/services.d`, and mounts `ufs:` at
   `/`. `vfsd` treats `mount("/")` as the live pivot: it installs a replacement mount table with the
   UFS root and every non-root entry (`/tmp` on ramfs, `/dev/null`, `/dev/zero`, `/dev/random`,
   `/dev/urandom`), then bumps the mount generation. Open files keep their targets.
3. The root service set and `/etc/rc.conf` are merged from UFS; `ROOT_PIVOTED` makes merged
   services execute from the pivoted root (`/bin/getty`), not `/root`. `raven_rt::spawn` falls back
   to the `file:` root when the bootstrap initfs does not carry an image (login and the shell).
4. The UFS payload ships a post-pivot `/etc/mounts` (`/ ufs:`) through the rootfs-spec overlay, so
   `df` shows `ufs: … /` and `mount` shows `ufs: on /`.

A failed probe or activation keeps the initfs root and reports the reason; no fake root and no
initfs-embedded UFS is used.

## Full demo (K193)

The bootable image is the full demo, not only a root pivot. The UFS payload carries the release
login/network userland (`brush`, `ed`, `df`, `mount`, `ping`, `ls`, `cat`), `/root`, and the same
static `lsblk`/`gpart`/`newfs` fixtures used by the S99 `ufs-demo`, plus their man pages.
`scripts/qemu-ufs.sh` runs `scripts/ufs-demo.sh --prepare-tools` before the image build.

```sh
make qemu-ufs                    # interactive: tools, ESP, 256 MiB UFS2 root, QEMU
```

At the `login: root` / `password: raven` prompt:

```text
cd /root
pwd                              # /root
lsblk                            # block0 ... disk; block0p1 ... part ufs-root
gpart show block:0
df -h /                          # ufs: 240M ...
mount                            # ufs: on /
ping 10.0.2.2
```

`newfs -h` prints its usage without formatting a disk. The K192 interactive prompt profile is
inherited-pending: `etc/brush/**` is owned by K192 and is not edited here.

## Commands

```sh
sh scripts/ufs-demo.sh --prepare-tools   # static disk-tool fixtures
cargo xtask image --arch x86_64 --profile qemu-ufs --output build/qtest/ufs-full.img
cargo xtask qtest --case ufs-full --arch x86_64 --no-build
cargo xtask qtest --case ufs-root --arch x86_64 --no-build
cargo xtask qtest --case ufs-root-persist --arch x86_64 --no-build   # after ufs-root
```

`make qemu-ufs` is `scripts/qemu-ufs.sh` with no flags: it builds the image and runs QEMU with the
ESP as the boot drive, the UFS root disk as virtio-blk and the QEMU user NIC.

## Evidence

`ufs-root` (30-160 s depending on the image build) reaches the login from UFS, and its transcript
contains `svcd: root pivoted to UFS at /`, the merged UFS service plan, `ufs: 1.9G … /` (df),
`ufs: on /` (mount) and `ping: 4 packets transmitted, 4 received, 0% packet loss`. The second boot
(`ufs-root-persist`) reads the marker written to the same root disk on the first boot. The full
transcript and the acceptance results are in `tickets/kevin/reports/K176.md`.
