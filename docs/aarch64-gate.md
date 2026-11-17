<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 QEMU release-parity gate

Recorded regression sweep of the aarch64 QEMU surface at commit `51e6d38f`
(`dave/A8`, from `origin/master`).

Command set:

```
cargo xtask qtest --case <case> --arch aarch64
```

| Case | Result |
|---|---|
| `kernel-early-boot-aarch64` | pass |
| `kernel-exc-aarch64` | pass |
| `kernel-paging-aarch64` | pass |
| `kernel-timer-aarch64` | pass |
| `sched-kernel-threads-aarch64` | pass |
| `aarch64-libc` | pass |
| `boot-uname-aarch64` | pass |
| `scheme-aarch64` | pass |
| `vfs-aarch64` | **fail** |
| `pipe-fork` | pass |
| `proc-death-cleanup` | pass |
| `trace-context` | pass |
| `time-entropy` | pass |

## Failing case

`vfs-aarch64` (profile `fsd-ufs-test`) fails in the K54 UFS probe, before any aarch64 kernel path:

```
fsd-ufs: probe: relative READLINK returned Status(-2)
process: child exited (1)
svcd: fsd-ufs-probe crash loop, giving up
svcd: settled
init exited (1)
```

The probe's relative `Readlink { path: b"fastlink" }` through vfsd returns `ENOENT`, so the probe
never prints `probe errors ok` and the expectations that follow. The same failure reproduces on
x86-64 (`cargo xtask qtest --case fsd-ufs --arch x86_64`, exit 1, identical `relative READLINK
returned Status(-2)`), so this is a shared vfsd/UFS relative-readlink regression, owned by the UFS
lane (mel), not `kernel/src/arch/aarch64/**`.

## Outcome

No `kernel/src/arch/aarch64/**` regression was found and no aarch64 kernel code changed. The one
failure is shared-path and is reported to the lead with its owning lane. This is QEMU parity only;
there is no Pi 4 board claim.
