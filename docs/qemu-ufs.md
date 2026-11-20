<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU UFS demo and preflight (K176/K193)

Status: **OBSERVED (QEMU).** The bootable demo is one GPT disk with the FAT ESP first and the
256 MiB UFS2 root second, converted to `build/x86_64/ravenbsd-ufs-demo/ravenbsd-ufs-demo.qcow2`.
The older two-disk ESP + root preflight is retained only for the `ufs-root` regression.

## 1. One-disk demo

```sh
make qemu-ufs
scripts/qemu-ufs.sh --build-only   # build only; prints the qcow2 path; no QEMU
```

This prepares the static disk tools, builds the release userland, assembles the one GPT disk,
converts it with `qemu-img -O qcow2`, and runs:

```text
build/x86_64/ravenbsd-ufs-demo/
├── ravenbsd-ufs-demo.qcow2
├── run.sh
└── README
```

The exact boot disk argument is one virtio-blk drive, forced to the modern device
(`0x1042`) that the guest `virtio-blk` driver matches:

```text
-global virtio-blk-pci.disable-legacy=on
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

The target-built `raven` admin client is staged at `/usr/sbin/raven`; `/usr/sbin` is in the
root PATH and `/bin/raven` is not staged.

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

`ufs-full` copies the demo qcow2 to its case image and boots it with the new `disk_if =
"virtio"` / `disk_format = "qcow2"` harness fields, so the guest sees exactly one modern
virtio-blk disk. `qemu-img check`/`info` are the qcow2 checks; the `ufs-full` transcript is the
observed qcow2 boot.

## 2. Two-disk preflight

The K176 `ufs-root` regression still uses the separate bootstrap ESP and root disk.
`cargo xtask image --arch x86_64 --profile qemu-ufs --output <path>` writes the two-disk
**preflight ESP** at `<path>` and the separate `build/x86_64/qemu-ufs-root.img`; it does not
write the demo disk to `--output`.

```sh
cargo xtask qtest --case ufs-root --arch x86_64
cargo xtask qtest --case ufs-root-persist --arch x86_64 --no-build
```

The boot transcript still contains `svcd: root pivoted to UFS at /`, `ufs: 240M … /`,
`ufs: on /` and `k176-persist`. The demo `ufs-full` case is self-building: its `pre` runs
`scripts/qemu-ufs.sh --build-only`, then the harness reads the qcow2 from the case `image`
field instead of rebuilding the profile.

## 2a. Session policy and logout (K234)

The demo uses its own session policy, not the release one-shot:

- `profiles/qemu-ufs.d/getty.manifest`: `restart = "always"`, `restart_delay_ms = 200`,
  `max_restarts = 1000`, needs `vfsd`;
- `profiles/qemu-ufs.d/svcd.conf`: no `exit_when_exits = getty`;
- `profiles/qemu-ufs.d/10-session.toml` stages those two files.

A normal `exit` returns to `login:` and accepts a second login. The release profile keeps
`exit_when_exits = getty` and `release-login` remains the one-shot `test-exit=1` gate. Evidence:

```sh
cargo xtask qtest --case ufs-logout --arch x86_64   # login, exit, login again; timeout end
cargo xtask qtest --case release-login --arch x86_64 # unchanged one-shot
```

The bounded `ufs-logout` case boots without `test-exit`, requires a second `login:` and a second
successful login, and ends at the harness timeout (K235 will add a clean shutdown). It does not
fake an init exit. See `docs/session.md` for the donor map.

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
