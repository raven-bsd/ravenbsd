<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K378 UFS release-gate evidence

Tree: `978899da` (`Merge lance/K325: RACK output/timer scheduling independent model`),
branch `mel/K378`, 2026-11-21 UTC.

Host: `Linux alien 5.19.0-50-generic #50-Ubuntu SMP PREEMPT_DYNAMIC`,
20 CPUs, QEMU 6.2.0 (Debian `1:6.2+dfsg-2ubuntu6.31`).

Result: **BLOCKED**. Two independent regressions make the release UFS gate red on
this tree; see `docs/ufs-regression.md` for the verdict table and last-good shas.

## Gate results

| Gate | Command | Cap | Exit | `duration_ms` | Result |
|---|---|---:|---:|---:|---|
| ufs-disk | `cargo xtask qtest --case ufs-disk --arch x86_64` | 480 | 0 | 293045 | PASS |
| ufs-disk-persist | `cargo xtask qtest --case ufs-disk-persist --arch x86_64 --no-build` | 300 | 0 | 47002 | PASS |
| ufs-disk-ufs1 | `cargo xtask qtest --case ufs-disk-ufs1 --arch x86_64 --no-build` | 300 | 0 | 61886 | PASS |
| ufs-root | `cargo xtask qtest --case ufs-root --arch x86_64 --no-build` | 300 | 1 | 69209 | FAIL |
| ufs-logout | `cargo xtask qtest --case ufs-logout --arch x86_64 --no-build` | 300 | 0 | 90185 | PASS |
| disk-tools | `cargo xtask qtest --case disk-tools --arch x86_64` | 480 | 1 | 290910 | FAIL |
| ufs-update | `cargo xtask qtest --case ufs-update --arch x86_64 --no-build` | 300 | 1 | 60574 | FAIL |
| ufs-update-persist | `cargo xtask qtest --case ufs-update-persist --arch x86_64 --no-build` | 90 | 1 | 55132 | FAIL |
| ufs-full (one-disk demo) | `cargo xtask qtest --case ufs-full --arch x86_64` | 480 | 1 | 222299 | FAIL |
| ufs-selftest | `make ufs-selftest` | 180 | 0 | ~1000 | PASS |
| check-fast | `make check-fast` | 900 | 0 | 131000 | PASS |

Caps: the ticket caps are 300 s (and 90 s for `ufs-update-persist`). On this host
a full image build alone measures 308 s (`ufs-disk.img`) and 169-258 s
(`qemu-ufs` ESP), so the build-bearing cases use 480 s with the measurement
recorded; every `--no-build` case uses the ticket cap. See
`/tmp/opencode/k378-image-*.log` and the `## Status` lines in the report.

## Failure 1: qemu-ufs cases end at `login:` instead of `init exited (0)`

`ufs-root`, `ufs-update`, `ufs-update-persist` and `ufs-full` match every
product expectation (root pivoted to UFS, `ufs: on /`, the update/persist
markers, disk partitions, network) and fail only
`^init exited \(0\)$`. After `exit 0` the serial returns to:

```text
RavenBSD 0.1 (x86_64) (tty:console)
login:
```

Cause: K234 (`1ba8b422`, merged 2026-11-20T17:54:30+01:00) made the
`qemu-ufs` getty `restart = "always"` and removed `exit_when_exits` from
`profiles/qemu-ufs.d/svcd.conf`, so the demo session intentionally returns to
`login:` and init never exits. The cases were not updated and their
`init exited (0)` expectation is now unsatisfiable. `ufs-logout` (expects a
harness timeout and two logins) passes, which confirms the session policy is
the intended product behaviour and the other four cases are the stale side.

Last verified good shas: `26e5aa38` (mel/K181) and `e5b7db89` (mel/K185) for
`ufs-root`; `b2fcee8f` (mel/K185) for `ufs-update`/`ufs-update-persist`. All
predate K234.

## Failure 2: disk-tools virtio IRQ conflict

`disk-tools` (release profile, virtio-blk scratch disk **and** virtio-net)
fails at `blockd running`:

```text
devd: driver virtio-blk msix unavailable: bad handle or file descriptor; using legacy irq
devd: driver virtio-blk irq line=11
virtio-blk: registered disk
devd: driver virtio-net msix unavailable: bad handle or file descriptor; using legacy irq
devd: driver virtio-net launch failed: irq: resource busy
devd: virtio-net: irq: resource busy
svcd: devd failed: exited before it became ready
...
blockd: waiting for disk:0: no such file or directory
svcd: blockd failed: readiness timeout
```

Both virtio functions fall back to legacy PCI IRQ line 11 (PIIX4 shared PIRQ);
the kernel claims a legacy GSI exclusively (`kernel/src/driver/irq.rs`
`gsi_owners::claim`), so the second driver gets `EBUSY`, `devd` exits before it
is ready, and the `disk:` scheme is never available to `blockd`.

This reproduces with a freshly rebuilt case image in the same run (see
`serial/disk-tools.log`), so it is not stale-image state. Last verified good sha:
`18700ffb` (merge of `bob/S110`, which added the gate).

## Artifacts

Serial/qtest logs and harness JSON: `serial/` and `build/`. Image build
transcripts: `build/k378-image-*.log`. SHA-256 of the logs: `SHA256SUMS`.

Large images referenced by path and hash:

| Image | SHA-256 |
|---|---|
| `build/x86_64/qemu-ufs-root.img` | `d9a5eb7e27f2adf877f324ca40d5e740ee0d7929ce7d4f77bd509e39e014d98a` |
| `build/fixtures/ufs-demo/disk.img` | `6c53213a2b60fd99a7749d89e6b04cc17f7a1c14b8eb8728c99f35f8a3962be6` |
| `build/fixtures/ufs-demo/disk-ufs1.img` | `975d1585ec205fc7cc1a550103887e9420afb3423a58efb5f885013dfc2763eb` |
| `build/x86_64/ravenbsd-ufs-demo/ravenbsd-ufs-demo.qcow2` | `a8ee1b3c31adabc7d616f1e8865f9273545d9c05de91bc97b6c9ed1e44decdc3` |
| `build/qtest/ufs-disk.img` | `df705ff24fec80935543e7c0d354a7876762ada2fb218b3f4e41f67bdef62d3e` |
| `build/qtest/ufs-root.img` | `9db6052fa1da611080cc769cd4a72ab44c00f5a19960f99636f4c6eb251ca951` |
| `build/qtest/disk-tools.img` | `5066fc1142597be89a3bc1bb062825d33909dbd66d967d2f7b10cb07f65c1615` |

## Reproduce

```sh
git switch mel/K378
cargo xtask image --arch x86_64 --profile qemu-ufs --output build/qtest/ufs-root.img
cargo xtask image --arch x86_64 --profile ufs-demo --output build/qtest/ufs-disk-persist.img
cp build/qtest/ufs-disk-persist.img build/qtest/ufs-disk-ufs1.img
cargo xtask image --arch x86_64 --profile qemu-ufs --output build/qtest/ufs-logout.img
cargo xtask image --arch x86_64 --profile release --output build/qtest/disk-tools.img
cargo xtask qtest --case ufs-disk --arch x86_64
cargo xtask qtest --case ufs-disk-persist --arch x86_64 --no-build
cargo xtask qtest --case ufs-disk-ufs1 --arch x86_64 --no-build
cargo xtask qtest --case ufs-root --arch x86_64 --no-build
cargo xtask qtest --case ufs-logout --arch x86_64 --no-build
cargo xtask qtest --case disk-tools --arch x86_64
cargo xtask qtest --case ufs-update --arch x86_64 --no-build
cargo xtask qtest --case ufs-update-persist --arch x86_64 --no-build
cargo xtask qtest --case ufs-full --arch x86_64
make ufs-selftest
make check-fast
```
