<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU UFS demo and preflight (K176/K193)

Status: **OBSERVED (QEMU).** The bootable demo is one GPT disk with the FAT ESP first and the
256 MiB UFS2 root second, converted to `build/x86_64/ravenbsd-ufs-demo/ravenbsd-ufs-demo.qcow2`.
The older two-disk ESP + root preflight is retained only for the `ufs-root` regression.

## 1. One-disk demo

```sh
make qemu-ufs
```

This prepares the static disk tools, builds the release userland, assembles the one GPT disk,
converts it with `qemu-img -O qcow2`, and runs:

```text
build/x86_64/ravenbsd-ufs-demo/
├── ravenbsd-ufs-demo.qcow2
├── run.sh
└── README
```

The exact boot disk argument is one virtio-blk drive:

```text
-drive file=ravenbsd-ufs-demo.qcow2,if=virtio,format=qcow2
```

`run.sh` uses OVMF pflash code/vars, KVM when `/dev/kvm` is writable, `-serial stdio`, and no
second root disk. Its default network is the K194 bidirectional helper:

```sh
sudo scripts/qemu-net.sh up       # ravenbr0/ravendemo0, NAT, dnsmasq
./run.sh
./run.sh --check                  # qemu-img, OVMF and helper; no boot
./run.sh --user-net               # documented non-bidirectional fallback
```

Root and a tap/bridge are required for the default network; without them `run.sh` refuses and
points at `--user-net`. The helper cleanup (`scripts/qemu-net.sh down`) runs on exit.

The GPT layout is:

- ESP/FAT: partition 1, LBA 2048, 450560 sectors (220 MiB);
- UFS2 root: partition 2, LBA 452608, 524288 sectors (256 MiB);
- the bootstrap `fsd-ufs` manifest selects partition 2 for the one-disk demo.

Guest recipe:

```text
login: root / raven
cd /root
lsblk                            # block0; block0p1 ESP; block0p2 ufs-root
gpart show block:0
newfs -h
df -h /                          # ufs: 240M ...
mount                            # ufs: on /
ping 10.0.2.2
```

The payload carries `brush`, `ed`, `df`, `mount`, `ping`, `ls`, `cat`, the static
`lsblk`/`gpart`/`newfs` and their man pages. K192's interactive prompt is inherited-pending;
`etc/brush/**` is not edited here.

`ufs-full` boots the same single GPT disk as the harness's one disk drive (the harness uses the
default IDE controller; `run.sh` uses virtio-blk). `qemu-img check`/`info` are the qcow2 checks.
No qcow2 boot is claimed in this report unless it was actually booted.

## 2. Two-disk preflight

The K176 `ufs-root` regression still uses the separate bootstrap ESP and root disk:

```sh
cargo xtask qtest --case ufs-root --arch x86_64 --no-build
cargo xtask qtest --case ufs-root-persist --arch x86_64 --no-build
```

The qemu-ufs image build emits `build/x86_64/qemu-ufs-preflight.img` (ESP) and
`build/x86_64/qemu-ufs-root.img` (one-partition UFS GPT). Copy the preflight ESP to the qtest
selector:

```sh
cp build/x86_64/qemu-ufs-preflight.img build/qtest/ufs-root.img
```

The boot transcript still contains `svcd: root pivoted to UFS at /`, `ufs: 240M … /`,
`ufs: on /` and `k176-persist`.

## 3. Evidence

Observed one-disk build (K193 rework):

```text
qemu-img check build/x86_64/ravenbsd-ufs-demo/ravenbsd-ufs-demo.qcow2
run.sh --check
ufs-full: pass
ufs-root: pass
```

The one-disk GPT sectors are 976929; `lsblk` shows the 230686720-byte ESP and the
268435456-byte `ufs-root` partition. No Pi 4 board claim and no real SD write.
