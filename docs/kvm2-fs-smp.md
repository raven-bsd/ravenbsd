<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# KVM2 `posix-fs-smp` setup-to-worker transition (S139)

Status: **OBSERVED at product commit `ec40b545`.** The r8-full2 KVM2 cap expiry is **not
reproduced** on a clean tree at `ec40b545`; the old image and the current one carry a
byte-identical userspace fixture. A bounded start-transition regression and in-run progress lines
now make the next occurrence classifiable. No VFS/ramfs lock fault and no kernel handoff were
identified.

## 1. The r8-full2 observation

The closeout at `82c9efa6` (kernel string `82c9efa6-dirty`; the dirty file was the uncommitted
runner/docs edit that became `e9c67db1`) stopped at cell 15:

```text
smp-closeout: [15] mode=kvm2 case=posix-fs-smp-kvm2
smp-closeout: FAIL mode=kvm2 case=posix-fs-smp-kvm2 rc=1 result=timeout cell_timeout=1
smp: 2 cpus online
vfsd: skipped /dev/null on null:: timed out
vfsd: skipped /dev/zero on zero:: timed out
vfsd: skipped /dev/random on rand:: timed out
smp-fs: services ok
smp-fs: setup ok
smp-closeout: stopped on the first failure; no retry
```

Cell cap 40 s, duration 39902 ms, image
`2793805c5edff4734cf08059afe6b4ebab0ba86e6ecfa0c6c782b54827ecb359`. Host telemetry for the
cell was load 0.84-1.09 with only the cell's QEMU/xtask/cargo; no external job was present. The
serial log stops after `setup ok` because the fixture prints latency lines only after **all**
workers finish, so the cap could not distinguish a stall from a slow run.

## 2. Identity and reproduction at `ec40b545`

`82c9efa6` is a merge whose second parent is `ec40b545`; the only source difference is
`docs/smp-closeout.md` and `scripts/smp-closeout.sh`. The userspace fixture packed into the
r8-full2 image and the fixture rebuilt at `ec40b545` are byte-identical:

```text
4b68e8b4ce7fced50419f798d60091747a1fcc7d9965c135753fbf72c8b6a73d  build/fixtures/posix-fs-smp/initfs.img
/init 82344 0755 bda208ed
/sbin/initfsd 70056 0755 3a04d808
/sbin/ramfsd 98728 0755 a66707d2
/sbin/vfsd 119208 0755 8be229d7
```

The exact acceptance run and the closeout context both pass at `ec40b545`:

```text
$ timeout 40 cargo xtask qtest --case posix-fs-smp-kvm2 --arch x86_64 --no-build
posix-fs-smp-kvm2: pass          # 22.7 s fresh, 24.4 s after the fixture change

$ timeout 60 sh scripts/smp-closeout.sh --case posix-fs-smp --mode kvm2 \
    --no-preflight --out /tmp/opencode/s139-kvm2-closeout
kvm2  posix-fs-smp-kvm2  rc=0  pass  24538  cap=40
```

The `/dev/null`, `/dev/zero` and `/dev/random` skips are normal for this fixture: it starts only
`initfsd`, `ramfsd` and `vfsd`, so vfsd exhausts its bounded readiness wait for the three absent
default schemes. The same lines appear in every passing KVM1/KVM2/KVM4 run and are not the hang.

## 3. Classification

- vfsd and ramfsd are single-threaded scheme servers. `VfsHandler` owns the `MountTable` and
  `OpenTable`, and the server dispatches one request at a time, so the mount path has no
  in-process lock that eight workers can deadlock. The mount/unmount cycles serialize behind the
  servers, which can make KVM2 slower than KVM1 but does not create a lock cycle.
- The failing log stopped before the first latency line, so it cannot distinguish worker progress
  from a stall. The closeout telemetry and the clean-tree reruns do not support a kernel or
  init/svcd readiness fault; there is no trace naming the kernel.
- The old run was 15 s slower than the KVM2 reruns on the same userspace fixture and the same
  kernel source. The evidence is consistent with a slow serialized run under the 40 s cap, not a
  reproducible VFS/ramfs lock or a kernel defect. This is why no handoff to Stuart/Kevin is made.

## 4. Bounded regression and diagnostics

`tests/posix/fs/posix-fs/src/bin/start.rs` is a dedicated eight-worker transition fixture. It
starts the three services, runs the stress setup, creates eight threads and requires every thread
to enter its entry function within 10 s. It prints `smp-start: workers started workers=8` and
`init exited (0)`; a stall prints the pending worker indexes and exits non-zero.

The case `tests/posix/fs/posix-fs/posix-fs-start-kvm2.toml` bounds the whole run to 30 s and
passes on KVM2:

```text
$ timeout 35 cargo xtask qtest --case tests/posix/fs/posix-fs/posix-fs-start-kvm2.toml \
    --arch x86_64 --no-build
posix-fs-start-kvm2: pass        # 20.1 s
smp-start: services ok
smp-start: setup ok
smp-start: workers started workers=8
smp-start: done
init exited (0)
```

The main fixture now prints `smp-fs: workers started workers=8` and a
`smp-fs: progress finished=N/8 pending=[...]` line every 5 s. A future cap kill preserves the
worker phases instead of ending at `setup ok`.

## 5. Full matrix at `ec40b545`

`timeout 600 sh scripts/smp-closeout.sh --max-load 5 --wait-for-idle 300 --out
/tmp/opencode/s139-full2` ran all 20 cells on the same commit; the first cold attempt was stopped
during cell 1's cold brush/image build, so the brush and profile images were warmed first.

| Mode | Cells | `posix-fs-smp` |
|---|---:|---:|
| TCG4 | 5/5 | 110.3 s / cap 140 s |
| KVM1 | 5/5 | 23.2 s / cap 40 s |
| KVM2 | 5/5 | 26.1 s / cap 40 s |
| KVM4 | 5/5 | 25.8 s / cap 40 s |

**20/20 pass.** No cell was retried and no cap was raised.

## 6. Donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| Mount table serialization | `vfs_lock`/`vfs_unlock` guard a per-vfs mount state (`illumos-gate@586678ac`, `uts/common/fs/vfs.c`). Behaviour only. | NetBSD `sys/kern/vfs_mount.c` (`mountlist_lock`, `vfs_busy`) is permissive BSD-2-Clause at `36eddeb`; not ported. FreeBSD `vfs_mount.c` is not in the sparse checkout (`1333444`). | Read-only reference; mount namespaces add their own locks. | vfsd is a single-threaded server, so this ticket adds no lock. The donor mount locks confirm the observed serialization is expected, not a deadlock source. |
| Readiness of absent `/dev` schemes | SMF dependency readiness is service-manager policy, not VFS. | No BSD file needed. | Read-only reference. | Keep the bounded vfsd readiness wait and the existing skip lines; they are normal for this fixture. |
| Test-only transition bound | Not applicable. | Not applicable. | Not applicable. | First-party test code; no donor code copied or translated. |
