<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD UFS demo (one disk)

`ravenbsd-ufs-demo.qcow2` is the full bootable demo: one GPT disk with the FAT ESP as partition 1
and the 256 MiB UFS2 root as partition 2. The ESP carries the loader, kernel and bootstrap initfs;
the UFS root carries the release login/network userland, `/root`, `lsblk`, `gpart`, `newfs` and
their man pages.

## Boot

```sh
./run.sh
```

`run.sh` attaches exactly one disk to the guest:

```text
-global virtio-blk-pci.disable-legacy=on
-drive file=ravenbsd-ufs-demo.qcow2,if=virtio,format=qcow2
```

The `-global` keeps the auto-created device modern (`0x1042`) so the guest `virtio-blk`
driver matches it.

It uses OVMF pflash code/vars, KVM when `/dev/kvm` is writable, `-serial stdio`, and no second root
disk. The default network is the K194 bidirectional tap/bridge helper:

```sh
sudo scripts/qemu-net.sh up     # ravenbr0/ravendemo0, NAT, dnsmasq
./run.sh
# run.sh calls scripts/qemu-net.sh down on exit
```

Root and a tap/bridge are required for the default network. Without them, `run.sh` refuses
clearly. The documented fallback is non-bidirectional (guest to host/internet only):

```sh
./run.sh --user-net
```

## Check

```sh
./run.sh --check
qemu-img check ravenbsd-ufs-demo.qcow2
qemu-img info ravenbsd-ufs-demo.qcow2
```

`--check` verifies `qemu-img`, the OVMF code/vars and the network helper without booting. It does
not need root.

## Guest

```text
login: root
password: raven
cd /root
lsblk
gpart show block:0
newfs -h
df -h /
mount
ping 10.0.2.2
```

The root disk is partition 2 because the ESP is partition 1. No Pi 4 board claim and no real SD
write; this is a QEMU image only.
