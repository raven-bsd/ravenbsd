<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Root mount semantics (K174)

`svcd` treats an image as root-capable when its service set contains `blockd`, `fsd-ufs` and
`vfsd`. That chain alone is not evidence of a **device**: the QEMU release profile stages the
servers but has no UFS root, and the old path still mounted `ufs:` at `/root`.

## The dangling mountpoint

`MountTable::mount("/root", "ufs:")` only records a mount. The root listing merges the names of
mounts below `/`, so `/root` appeared even though the `ufs:` scheme could not serve it. `readdir`
listed `root/`, every `lstat` failed, and `ls -alh /` printed:

```text
ls: cannot access '/root': No such file or directory
...
d????????? ? ?    ?       ?            ? root/
```

## Contract

1. Before creating the mount, probe the root scheme's required content:
   `ufs:/etc/services.d` (the activation path reads it anyway). A probe failure means there is no
   real root device; `svcd` reports `/root: no UFS root device (...)` and starts the initfs plan
   **without** mounting, so `/root` never enters the namespace.
2. If the probe succeeds but a later activation step fails (services, `rc.conf` or the service
   merge), unmount `/root` before falling back. `vfsd` removes the mount entry and bumps the mount
   generation, so the root listing cannot keep the stale name.
3. A root-capable image keeps the same behaviour: the probe succeeds, `/root` mounts, the root
   service set and `/etc/rc.conf` are merged, and the root one-shots run through `vfsd`.
4. No fake root, no cached UFS state and no kernel/UFS driver change is involved.

`tests/qtest/cases/release-login.toml` runs `ls -alh /` and forbids both
`cannot access '/root'` and a `d?????????` entry, so the regression is bounded.

The implementation is in `userland/init/src/boot.rs` (`activate_root` / `activate_mounted_root`);
the reproducer transcript, the fixed serial and the root-capable image evidence are in
`tickets/kevin/reports/K174.md`.

**OBSERVED** on the release image `96f8b367…`: the serial reports
`svcd: root: /root: no UFS root device (no such file or directory); starting the initfs plan`,
`release-login` passes with a clean `ls -alh /`, and `hw-boot` still mounts `/root` and runs the
root services. The full transcript is in the K174 report.
