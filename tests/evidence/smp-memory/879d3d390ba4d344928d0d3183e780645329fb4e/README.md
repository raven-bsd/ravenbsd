<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K398 SMP memory matrix evidence — `879d3d390ba4d344928d0d3183e780645329fb4e`

Post-NMI re-verification of the merged S85b SMP memory/paging/TLB matrix. This is verification
only: no kernel, vmem, S159 fix path or S85b harness source was changed. The evidence branch is
`mark/K398`, based on `origin/master` `879d3d390ba4d344928d0d3183e780645329fb4e`. The S159 NMI
delivery fix (`9078d345`) is an ancestor of this tree.

## Host and build

- Date: 2026-11-22 (UTC).
- Host: 20-CPU shared Linux host; `uptime` load average was **23.65** before the matrix and
  **35.16** after it (1-minute). This is the main difference from K379, which ran at 93–122.
- QEMU: 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31).
- Kernel image: `build/smp-memory/smp-memory.img`, SHA-256
  `bb50b1ee7eefaeb6e321a7816c9c55bc40321f3b9fb4278d86519e5c4f982b54`.
- Kernel ELF: `build/x86_64/kernel-debug.elf`, SHA-256
  `e227b982f7d1304aff2860a509c927fb309c3fbf688df878eb619fb023f5bc1c`.
- Build command: `run.sh` builds `tests/smp/memory` and runs
  `cargo xtask image --arch x86_64 --profile base --cmdline 'console=ttyS0 test-exit=1 smp_user=on smp-user-trace'`
  before the iterations.

## Matrix

Ten iterations per mode, as the K398 contract requires. The harness modes are `tcg`, `kvm2` and
`kvm4`; the ticket's shorthand `kvm` is not a mode, so both KVM widths are recorded separately.

| Mode | QEMU SMP | Command | Exit | Runs | TLB timeouts | `tlb_stale` | Memory result | Duration worst / p90 / avg |
|---|---:|---|---:|---:|---:|---|---|---|
| TCG | 4 | `timeout 600 sh tests/smp/memory/run.sh tcg --iterations 10` | 0 | 10/10 | **0** | 0 every run | `smp-memory: ok` every run | 12,916 / 11,856 / 8,181 ms |
| KVM | 2 | `timeout 400 sh tests/smp/memory/run.sh kvm2 --iterations 10` | 0 | 10/10 | **0** | 0 every run | `smp-memory: ok` every run | 5,049 / 4,924 / 4,663 ms |
| KVM | 4 | `timeout 400 sh tests/smp/memory/run.sh kvm4 --iterations 10` | 0 | 10/10 | **0** | 0 every run | `smp-memory: ok` every run | 10,792 / 7,719 / 5,731 ms |

Per-iteration data: `matrix.tsv`.

## Comparison with K379

K379 ran the same 10/10 matrix at `b8169cce` before the NMI delivery fix. K398 ran it at
`879d3d39` after the fix (`9078d345`).

| Mode | K379 runs | K379 TLB | K379 worst / avg | K398 runs | K398 TLB | K398 worst / avg | Difference |
|---|---:|---:|---|---:|---:|---|---|
| TCG 4 | 10/10 | 0 | 41,485 / 29,772 ms | 10/10 | 0 | 12,916 / 8,181 ms | pass counts and TLB counts identical; K398 is ~3.6x faster |
| KVM 2 | 10/10 | 0 | 40,095 / 25,472 ms | 10/10 | 0 | 5,049 / 4,663 ms | pass/TLB identical; K398 is ~5.5x faster |
| KVM 4 | 10/10 | 0 | 48,184 / 24,643 ms | 10/10 | 0 | 10,792 / 5,731 ms | pass/TLB identical; K398 is ~4.3x faster |

The duration difference tracks the host load difference (K379: 93–122; K398: 23–35) and is not
attributed to the NMI fix. The relevant post-fix comparison is: **no regression, no TLB timeout,
no stale line**; the NMI-specific IF=0 delivery proof is S159's
`selftest: smp tlb if0 ok elapsed_ms=1` and its mutation, not this stress matrix.

## TLB vs memory separation

- Every run's serial log was scanned for the literal `smp: tlb shootdown timeout`; the count is
  **0 for all 30 runs**.
- Every run's summary line contains `tlb_stale=0`.
- Every run contains `smp-memory: ok` and the stress summary
  `alloc_ops=256 fault_pages=512 maps=32 forks=4 tlb_rounds=64 tlb_stale=0`.
- No memory/paging failure, no panic/exception, no timeout line and no retry. The harness stops on
  the first forbidden line; none occurred.

## Verdict

**PASS** for the post-NMI 10/10 matrix on TCG `-smp 4`, KVM `-smp 2` and KVM `-smp 4` at
`879d3d39...`. TLB timeouts are counted separately and are zero; the memory/paging stress is
clean; the result matches K379 with no regression.

## Artifacts

- `matrix.tsv` — all 30 iteration rows.
- `tcg/summary.txt`, `kvm2/summary.txt`, `kvm4/summary.txt` — runner aggregates.
- `<mode>/run-N.log` — serial log for each iteration.
- `<mode>/run-N.json` — qtest result and duration.
- `<mode>/command.log` — full command stdout/stderr and build output.
- `image-sha256.txt` — image and kernel ELF hashes.
- `SHA256SUMS` — hashes of every evidence file.

Larger build artifacts stay in the product clone at `build/smp-memory/smp-memory.img` and
`build/x86_64/kernel-debug.elf`; their hashes are above.
