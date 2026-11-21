<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# UFS release gate — regression record

Date: 2026-11-21 (UTC) · Tree: `978899da` (`Merge lance/K325: RACK output/timer
scheduling independent model`) · Branch: `mel/K378` · Host: QEMU 6.2.0,
x86_64, 20 CPUs.

Verdict: **BLOCKED**. The UFS read-write and persistence paths are green on this
tree, but two independent regressions make the release gate red: four
`qemu-ufs` cases can no longer observe `init exited (0)`, and `disk-tools` fails
on a virtio legacy-IRQ conflict. Evidence and exact serials:
`tests/evidence/ufs/978899da/`.

## Latest verdict — merged master `b04e71ad` (2026-11-21)

Re-verified on `origin/master` `b04e71ad` (merged into `mel/K378` at
`5a59da2e`; source identical to `b04e71ad`). Evidence:
`tests/evidence/ufs/b04e71ad/`.

Result: **BLOCKED**. UFS is green; the gate is red on three non-UFS defects.

| Gate | Result | `duration_ms` | Evidence |
|---|---|---:|---|
| `ufs-disk` | PASS | 258291 | `serial/ufs-disk.log` |
| `ufs-disk-persist` | PASS | 47346 | `serial/ufs-disk-persist.log` |
| `ufs-disk-ufs1` | PASS | 98607 | `serial/ufs-disk-ufs1.log` |
| `ufs-root` | FAIL | 79891 | `serial/ufs-root.log` |
| `ufs-logout` | FAIL | 90303 | `serial/ufs-logout.log`, `serial/ufs-logout-r2.log` |
| `disk-tools` | FAIL | 134473 | `serial/disk-tools.log` |
| `ufs-update` | FAIL | 65241 | `serial/ufs-update.log` |
| `ufs-update-persist` | FAIL | 73704 | `serial/ufs-update-persist.log` |
| `ufs-full` | FAIL | 408023 | `serial/ufs-full.log` |
| `make ufs-selftest` | PASS | ~1000 | `serial/ufs-selftest.log` |
| `make check-fast` | PASS | 22000 | `serial/check-fast.log` |

The three defects, none of them UFS:

1. **K234 one-shot expectation** — `ufs-root`, `ufs-update`,
   `ufs-update-persist` and `ufs-full` match every product step and stop only at
   the post-`exit 0` `init exited (0)` expectation; the demo getty restarts by
   design.
2. **K374 login banner** — `ufs-logout` completes both logins and returns to
   `login:`, but the expected `RavenBSD 0.1 (x86_64) (tty:console)` line is
   replaced by the styled boot screen ending in
   `Welcome to RavenBSD 0.1 x86_64` (`userland/init/src/bootscreen.rs:119`,
   merged `7971d65f`). The case expectation is stale.
3. **virtio legacy IRQ** — `disk-tools` boots with virtio-blk and virtio-net on
   the same legacy IRQ 11; the second claim returns `EBUSY`, `devd` exits before
   ready, and `blockd` never sees `disk:0`.

One transient: the first `ufs-logout` boot hit `fsd-ufs: readiness timeout` at
60 s under host load 16-21 with four other QEMU guests; the re-run pivoted and
completed both logins, then failed only on the K374 banner. Recorded in
`serial/ufs-logout.log`.

## Previous run — `978899da`

## Gates

| Gate | Result | `duration_ms` | Evidence |
|---|---|---:|---|
| `ufs-disk` (first boot, write, disk tools) | PASS | 293045 | `serial/ufs-disk.log` |
| `ufs-disk-persist` (second boot, read marker) | PASS | 47002 | `serial/ufs-disk-persist.log` |
| `ufs-disk-ufs1` (UFS1 mount/write/read) | PASS | 61886 | `serial/ufs-disk-ufs1.log` |
| `ufs-root` (UFS2 root pivot, rw `/`, marker) | FAIL | 69209 | `serial/ufs-root.log` |
| `ufs-logout` (exit returns to `login:`) | PASS | 90185 | `serial/ufs-logout.log` |
| `disk-tools` (scratch GPT, newfs, mount, read) | FAIL | 290910 | `serial/disk-tools.log` |
| `ufs-update` (in-place update, boot) | FAIL | 60574 | `serial/ufs-update.log` |
| `ufs-update-persist` (second update, boot) | FAIL | 55132 | `serial/ufs-update-persist.log` |
| `ufs-full` (one-disk demo, rw root, tools) | FAIL | 222299 | `serial/ufs-full.log` |
| `make ufs-selftest` | PASS | ~1000 | `serial/ufs-selftest.log` |
| `make check-fast` | PASS | 131000 | `serial/check-fast.log` |

Every gate ran with an explicit foreground `timeout`. The ticket caps are 300 s
(90 s for `ufs-update-persist`); a full image build alone measures 308 s
(`ufs-disk.img`) and 169-258 s (`qemu-ufs` ESP) on this host, so the
build-bearing `ufs-disk`, `disk-tools` and `ufs-full` cases use 480 s with that
measurement recorded. All `--no-build` cases use the ticket cap. The images are
listed with SHA-256 in `tests/evidence/ufs/978899da/README.md`.

## Regression 1: `qemu-ufs` cases never reach `init exited (0)`

`ufs-root`, `ufs-update`, `ufs-update-persist` and `ufs-full` match every
product expectation — `svcd: root pivoted to UFS at /`, `ufs: on /`, the
`k176-persist`/`k185-*` markers, the partition table, network — and fail only
the final `^init exited \(0\)$`. After `exit 0` the console returns to
`login:`.

K234 (`1ba8b422`, merged 2026-11-20T17:54:30+01:00) made the `qemu-ufs` getty
restart on exit and removed `exit_when_exits` from the demo `svcd.conf`. That is
the intended demo behaviour, and `ufs-logout` proves it. The four cases above
still carry the older one-shot expectation and can never pass against the
current profile. They need their own one-shot boot profile (or an expectation
update in a separate ticket); this verification ticket does not patch them.

Last verified good shas (all before K234): `26e5aa38` (K181) and `e5b7db89`
(K185) for `ufs-root`; `b2fcee8f` (K185) for `ufs-update` and
`ufs-update-persist`.

## Regression 2: `disk-tools` virtio IRQ conflict

With a scratch virtio-blk disk and virtio-net attached, both functions fall
back from MSI-X to legacy PCI IRQ line 11. The first driver
(`virtio-blk`) claims the legacy GSI; the second (`virtio-net`) gets
`irq: resource busy`, `devd` exits before it becomes ready, the `disk:` scheme
is unavailable, and `blockd` times out after 550 attempts. The failure
reproduces with a case image rebuilt in the same run, so it is not stale-image
state.

Observed serial:

```text
devd: driver virtio-blk msix unavailable: bad handle or file descriptor; using legacy irq
devd: driver virtio-blk irq line=11
virtio-blk: registered disk
devd: driver virtio-net msix unavailable: bad handle or file descriptor; using legacy irq
devd: driver virtio-net launch failed: irq: resource busy
svcd: devd failed: exited before it became ready
blockd: waiting for disk:0: no such file or directory
```

Last verified good sha: `18700ffb` (merge of `bob/S110`, which added the gate).

## What is verified green

* UFS2 read-write mount of the demo disk, file write, read-back and unmount
  (`ufs-disk`).
* Persistence across reboot: the `s99-persist` marker is written before the
  reboot and read after it (`ufs-disk-persist`).
* The UFS1 variant of the same path (`ufs-disk-ufs1`).
* Disk tools see the GPT partition (`block0p1 16777216 32768 part ufs-root`,
  `gpart show block:0`) and the UFS2 root disk pivots `/` read-write with
  `df`/`mount` agreeing (`ufs-root`, before the final exit step).
* One-disk demo: root pivots to the UFS2 partition, disk tools see the ESP and
  `ufs-root` partitions, `df`/`mount`/`ping` are green (`ufs-full`, before the
  final exit step).
* In-place update writes and boots `k185-first`, and the second update boots
  `k185-second` (`ufs-update`, `ufs-update-persist`, before the final exit
  step).
* `raven-ufs` refusals and image hashes (`make ufs-selftest`) and the host gate
  (`make check-fast`).

## Rework round 1 re-run (2026-11-21)

Only the five failed cases were re-run on `978899da` (evidence:
`tests/evidence/ufs/978899da/rework/`). Classification first, from the recorded
serials: none of the five is a boot timeout and none shows a UFS error; four are
the stale `init exited (0)` expectation and one is the `devd`/`blockd` IRQ
failure. The confirming runs reproduce exactly that:

| Case | Re-run result | First failing line |
|---|---|---|
| `ufs-root` | FAIL 112 s | `step 23 (^init exited \(0\)$) failed` |
| `ufs-update` | FAIL 65 s | `step 15 (^init exited \(0\)$) failed` |
| `ufs-update-persist` | FAIL 61 s | `step 10 (^init exited \(0\)$) failed` |
| `disk-tools` | FAIL 135 s | `step 7 (^blockd running [0-9]+$) failed` |
| `ufs-full` | FAIL 124 s | `step 32 (^init exited \(0\)$) failed` |

One intermediate `ufs-update` boot failed after the pivot with `login: unable to
start the session shell` / `svcd: getty failed: input/output error`. That boot
ran while a QEMU from an earlier cap-expired attempt was still alive; after
killing it and rebuilding the baseline root disk, the same case booted
`k185-first` and stopped only at `init exited (0)`. The transient is recorded in
`rework/ufs-update-r2.log`; it did not reproduce on the clean boot.

The verdict stays **BLOCKED**: the two deterministic gate defects above are not
UFS regressions, so re-running cannot turn the gate green. They need a one-shot
demo boot profile (or an expectation update) and a virtio IRQ/MSI-X fix.

## Follow-up owners

* Case expectation vs K234 session policy: the `qemu-ufs` one-shot boot gate
  cases (`ufs-root`, `ufs-update`, `ufs-update-persist`, `ufs-full`).
* virtio legacy IRQ sharing / MSI-X fallback in `devd` + `kernel/src/driver/irq.rs`
  for the two-device `disk-tools` case.
