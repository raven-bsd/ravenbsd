<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU UFS root preflight (K176)

Goal: a QEMU image whose `/` is a real UFS2 partition staged from the release rootfs payload, with
the initfs as bootstrap only. K176 is **BLOCKED at the driver stage**; this document records the
design and the exact handoff.

## Pivot design

1. Bootstrap initfs services: `blockd`, `fsd-ufs`, `vfsd` plus the core scheme servers and `consd`.
   The profile stages `/etc/raven/root-mode` with `pivot`.
2. `svcd` probes `ufs:/etc/services.d`, then mounts `ufs:` at `/`. `vfsd` treats a mount at `/` as
   the live root pivot: it builds a replacement mount table with the new root and every non-root
   mount (`/tmp`, `/dev/null`, `/dev/zero`, `/dev/random`, ...), then bumps the mount generation.
   Open files keep their targets; new paths resolve to UFS.
3. `svcd` reads `/etc/services.d` and `/etc/rc.conf` from the pivoted root, merges them into the
   supervisor and starts the UFS service set. Login/getty, the shell and the tools then come from
   UFS. A failed merge remounts the initfs root.
4. `/dev` and `/tmp` stay valid through the mount table; `/tmp` remains the ramfs mount. Console
   and process handles are untouched by the pivot.

Implemented groundwork in this branch:

* `userland/servers/vfsd/src/pivot.rs`: `pivot_root(table, scheme)` + host tests.
* `userland/servers/vfsd/src/main.rs`: `Call::Mount { path: "/" }` installs the replacement table.
* `userland/init/src/boot.rs`: root-mode marker, UFS probe, `mount("/", "ufs:")`, UFS service
  merge, and initfs rollback.

## Blocker: one PCI driver per boot

The image needs **both** a virtio-blk root and a virtio-net NIC in one boot (login/ls/df/mount from
UFS and `ping 10.0.2.2`). The driver manager launches exactly one:

* `userland/servers/devd/src/main.rs`: `match_driver(&devices, &drivers)` returns one
  `(device, driver)` and the production path launches it once (`cycles = 1`).
* `profiles/ufs-demo.d/drivers.conf` states the limitation: only virtio-blk is launched and the
  network driver is disabled ("devd currently matches one PCI function per boot, and the UFS disk
  is the demo's hardware dependency").
* A second devd service cannot select a different `/etc/drivers.toml` and would re-match the first
  matching function.

This is a **devd/virtio driver-manager change outside K176's Owns**. Needed: launch every matched
`(device, driver)` in one devd instance (each driver publishes its own scheme), or give devd a
per-instance driver selector and stage two bootstrap services. With that, the remaining pieces are
the qemu-ufs payload/boot profiles, the ESP+UFS image assembly, `scripts/qemu-ufs.sh` /
`make qemu-ufs`, the `ufs-root`/`ufs-root-persist` qtests, and the five validation gates.

No fake root and no initfs-embedded UFS is used. The full stage evidence is in
`tickets/kevin/reports/K176.md`.
