<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K378 UFS release-gate evidence — merged master b04e71ad

Tree: `b04e71ad` (`Merge jerry/K403: raven-pi4-platd host test link fails with
duplicate _start`), merged into `mel/K378` at `5a59da2e`; the merged source is
identical to `b04e71ad`. Date: 2026-11-21 (UTC).

Host: `Linux alien 5.19.0-50-generic`, 20 CPUs, QEMU 6.2.0
(Debian `1:6.2+dfsg-2ubuntu6.31`), host load 13-25 with other minions' QEMU
guests running.

Result: **BLOCKED**. UFS read-write, UFS1 and persistence are green; the release
gate is red on three non-UFS gate defects (K234 exit expectation x4, K374 login
banner x1, virtio legacy-IRQ conflict x1). No UFS error was observed.

## Gates

| Gate | Exit | Result | `duration_ms` / wall | First failing line |
|---|---:|---|---|---|
| ufs-disk | 0 | PASS | 258291 / 258 s | — |
| ufs-disk-persist | 0 | PASS | 47346 / 48 s | — |
| ufs-disk-ufs1 | 0 | PASS | 98607 / 99 s | — |
| ufs-root | 1 | FAIL | 79891 / 81 s | `step 23 (^init exited \(0\)$) failed` |
| ufs-logout | timeout | FAIL | 90303 / 90 s | `^RavenBSD 0\.1 \(x86_64\) \(tty:console\)$` unmatched |
| disk-tools | 1 | FAIL | 134473 / 135 s | `step 7 (^blockd running [0-9]+$) failed` |
| ufs-update | 1 | FAIL | 65241 / 66 s | `step 15 (^init exited \(0\)$) failed` |
| ufs-update-persist | 1 | FAIL | 73704 / 74 s | `step 10 (^init exited \(0\)$) failed` |
| ufs-full | 1 | FAIL | 408023 / 409 s | `step 32 (^init exited \(0\)$) failed` |
| make ufs-selftest | 0 | PASS | ~1 s | — |
| make check-fast | 0 | PASS | 22 s | — |

Caps: ticket 300 s (90 s for `ufs-update-persist`). Build-bearing cases use
480 s because full image assembly on this host measures 204-308 s; every
`--no-build` case uses the ticket cap. `ufs-logout` was re-run once (150 s cap)
after a transient `fsd-ufs: readiness timeout` on the first boot.

## The three gate defects (none is UFS)

1. **K234 one-shot expectation.** `ufs-root`, `ufs-update`, `ufs-update-persist`
   and `ufs-full` match root pivot, `ufs: on /`, the update/persist markers,
   disk tools and network, then `exit 0` returns to `login:` and init never
   exits. The demo getty is `restart = "always"` by design (K234); the cases
   still expect `init exited (0)`.

2. **K374 login banner.** `ufs-logout` completes both logins and returns to
   `login:`, but the expected getty banner `RavenBSD 0.1 (x86_64)
   (tty:console)` is gone. K374 (`7971d65f`, `f17b276f`) replaced it with the
   styled boot screen ending in `Welcome to RavenBSD 0.1 x86_64`
   (`userland/init/src/bootscreen.rs:119`). The case expectation is stale.

3. **virtio legacy IRQ (`disk-tools`).** Both virtio functions fall back from
   MSI-X to legacy IRQ 11; `virtio-blk` claims it, `virtio-net` gets
   `irq: resource busy`, `devd` exits before ready, and `blockd` never finds
   `disk:0`:

   ```text
   devd: driver virtio-blk irq line=11
   virtio-blk: registered disk
   devd: driver virtio-net msix unavailable: bad handle or file descriptor; using legacy irq
   devd: driver virtio-net launch failed: irq: resource busy
   svcd: devd failed: exited before it became ready
   blockd: waiting for disk:0: no such file or directory
   ```

## Transient observed

The first `ufs-logout` boot (`serial/ufs-logout.log`) hit
`svcd: fsd-ufs failed: readiness timeout` at 60 s while host load was 16-21 and
four other QEMU guests were running; the case then timed out at 90 s. The re-run
(`serial/ufs-logout-r2.log`) pivoted to the UFS root and accepted both logins,
then failed only on the K374 banner. This is recorded as a boot-readiness
observation, not used to explain away the deterministic banner failure.

## Artifacts

`serial/` holds the qtest summaries and full boot serials; `build/` holds the
harness JSON and image-build logs; `SHA256SUMS` covers every small file.

| Image | SHA-256 |
|---|---|
| `build/x86_64/qemu-ufs-root.img` | `836227c3d4e7f10997d21682e52b3e212b56d163554df508af411be3a710f485` |
| `build/fixtures/ufs-demo/disk.img` | `6c53213a2b60fd99a7749d89e6b04cc17f7a1c14b8eb8728c99f35f8a3962be6` |
| `build/fixtures/ufs-demo/disk-ufs1.img` | `975d1585ec205fc7cc1a550103887e9420afb3423a58efb5f885013dfc2763eb` |
| `build/x86_64/ravenbsd-ufs-demo/ravenbsd-ufs-demo.qcow2` | `2963b6bae68c38aa7c45d9acbf7b04f51faaf0e6d04c469416f4791d4b655930` |
| `build/qtest/ufs-disk.img` | `8785750288c0edf1a33ec33911570793d4d45b9a983d478eb2d071a90e37e603` |
| `build/qtest/ufs-root.img` | `65809090f2688af65feabd2d21f19d57ee6b365ca6e3e87c454164909b1c2cb1` |
| `build/qtest/disk-tools.img` | `b060027026db89bdc65bf23d2eb173c69da421c72cc5427c5dc42923d9d7daaa` |
