<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K407: posix-fs-smp-kvm4 10x

Clean-build KVM `-smp 4` sample on the merged tree `0b3bb7d1f` (no dirty suffix), 10 runs.
Each run is `cargo xtask qtest --case posix-fs-smp-kvm4 --arch x86_64`.

| Run | Exit | Duration s | Host load1 | TLB/fault lines | Result |
|---:|---:|---:|---:|---:|---|
| 1 | 0 | 32 | 18.77 | 0 | pass |
| 2 | 0 | 49 | 16.13 | 0 | pass |
| 3 | 0 | 28 | 21.47 | 0 | pass |
| 4 | 0 | 32 | 16.21 | 0 | pass |
| 5 | 0 | 34 | 15.54 | 0 | pass |
| 6 | 0 | 32 | 16.43 | 0 | pass |
| 7 | 0 | 30 | 14.42 | 0 | pass |
| 8 | 0 | 32 | 13.68 | 0 | pass |
| 9 | 0 | 46 | 16.89 | 0 | pass |
| 10 | 0 | 30 | 21.26 | 0 | pass |

p50 32 s, p90 46 s, max 49 s. Every guest serial contains
`RavenBSD kernel 0.1 (x86_64) 0b3bb7d1f 2026-11-21`, `smp-fs: race successes=50/50`,
`smp-fs: done workers=8 iterations=50`, `init exited (0)`, and no `tlb`, `panic`, `EXCEPTION`
or `smp-fs: failed` line. `meta.tsv` is the machine-readable run table; `SHA256SUMS` pins every
artifact.

The tree was clean during the image build; the kernel banner has no `-dirty` suffix. An earlier
preliminary sample built while the evidence directory was untracked reported
`0b3bb7d1f-dirty` and is not used here.
