<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# UFS QEMU demo (S99)

The UFS demo boots the normal release login/network composition plus the UFS stack, with a
virtio-blk disk created on the host by `tools/ufs-mkfs`. It proves the demo path end to end:

1. `lsblk` and `gpart show block:0` see the disk and its `ufs-root` GPT partition.
2. `mount ufs: /mnt` mounts partition 1 read-write.
3. The shell creates `/mnt/s99-demo.txt`, reads it back and unmounts.
4. A second QEMU boot mounts the same disk and reads the same file.

The normal `release` profile is unchanged and still boots without a disk.

## Disk geometry

The default disk is a 16 MiB UFS2 payload wrapped in the deterministic
`raven-k54-mkimage` GPT:

- payload: 16 MiB (32768 sectors of 512 bytes)
- GPT first usable LBA: 34; payload at LBA 34; partition name `ufs-root`
- whole disk: 32835 sectors (16,811,520 bytes)
- `ufs-mkfs`: `-O 2 -b 4096 -f 512` (UFS1 variant: `-O 1`)
- no journal; fsd-ufs serves it read-write

The UFS1 case uses the same geometry with `ufs-mkfs -O 1`.

## Run

From the repository root:

```sh
sh scripts/ufs-demo.sh
```

The script:

- creates `build/fixtures/ufs-demo/disk.img` (UFS2) and `disk-ufs1.img` (UFS1);
- runs `cargo xtask qtest --case ufs-disk --arch x86_64` (first boot, write);
- runs `cargo xtask qtest --case ufs-disk-persist --arch x86_64` (second boot, read);
- runs `cargo xtask qtest --case ufs-disk-ufs1 --arch x86_64` (UFS1);
- checks all three transcripts contain the `s99-persist` marker.

Individual cases:

```sh
cargo xtask qtest --case ufs-disk --arch x86_64
cargo xtask qtest --case ufs-disk-persist --arch x86_64
cargo xtask qtest --case ufs-disk-ufs1 --arch x86_64
```

## Interactive QEMU demo

One command boots the full `ufs-demo` image with the UFS2 disk attached and leaves the serial
console interactive:

```sh
sh scripts/qemu-ufs.sh
```

The first run prepares `build/fixtures/ufs-demo/disk.img` if it is missing and builds
`build/ufs-demo/ufs-demo.img` for the current commit; later runs reuse the disk and the image.
`UFS_DEMO_REBUILD=1` recreates both. The script prints the image, disk, commit and transcript
paths before boot. The guest recipe is:

```text
login: root / raven
lsblk
gpart show block:0
mount ufs: /mnt
echo hello > /mnt/demo.txt
cat /mnt/demo.txt
umount /mnt
exit
```

`lsblk` must show `block0p1 ... ufs-root`; `gpart show block:0` shows the GPT scheme. Run
`sh scripts/qemu-ufs.sh` a second time **without** recreating the disk, mount `ufs: /mnt` again
and `cat /mnt/demo.txt` to show the file survives. The image is built with
`console=ttyS0 test-exit=1` (the same serial-exit contract as `ufs-disk`); the guest `exit 0`
maps QEMU's `isa-debug-exit` success status 33 to shell status 0; any other status is propagated.

Environment overrides: `UFS_DEMO_LOG` (default `build/ufs-demo/qemu.log`), `UFS_DEMO_IMAGE`
(default `build/ufs-demo/ufs-demo.img`), `UFS_DEMO_REBUILD`, `RAVEN_OVMF_CODE`,
`RAVEN_OVMF_VARS`. The script uses the same `-nic none` + virtio-net and virtio-blk arguments as
the `ufs-disk` qtest; it does not change the qtest cases or the release profile.

The first case's `pre` calls `scripts/ufs-demo.sh --prepare`, so the disk is recreated for each
first-boot run. The persistence case has no `pre`: it must run after `ufs-disk` so it sees the
written disk.

## Transcripts and images

- disk: `build/fixtures/ufs-demo/disk.img`
- UFS1 disk: `build/fixtures/ufs-demo/disk-ufs1.img`
- first boot: `build/qtest/ufs-disk.log` and `build/qtest/ufs-disk.json`
- second boot: `build/qtest/ufs-disk-persist.log` and `.json`
- UFS1: `build/qtest/ufs-disk-ufs1.log` and `.json`

## Profile

`profiles/ufs-demo.toml` extends `shell` and adds the release login/getty, network, disk tools,
and `profiles/ufs-demo.d/*`:

- `drivers.conf`: virtio-blk (1af4:1042) and virtio-net (1af4:1041)
- `devd.manifest`: `scheme:disk`, `scheme:nic`, `scheme:dev`
- `blockd.manifest`: `BLOCKD_DISK=disk:0`
- `fsd-ufs` service: `block:0` partition 1, read-write
- `vfsd.manifest`: ordered after `scheme:ufs`

## Guest newfs

S98 supplies the guest `newfs` tool. The first demo boot verifies the host-formatted disk, as the
ticket allows. Once the S98 guest formatter path is complete, the same disk can be reformatted in
the guest with:

```sh
newfs -O 2 block:0
mount ufs: /mnt
```

The host `ufs-mkfs` path remains the deterministic CI path.

## Demos repository

`ravenbsd-demos` can wrap the cases directly:

```sh
cargo xtask qtest --case ufs-disk --arch x86_64
cargo xtask qtest --case ufs-disk-persist --arch x86_64
cargo xtask qtest --case ufs-disk-ufs1 --arch x86_64
```

or run `sh scripts/ufs-demo.sh` from the main checkout after building the profile.
