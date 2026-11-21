<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K379 SMP memory matrix evidence — `b8169cce429177dc82f207e18b695dbcf204437d`

Dated re-verification of the merged S85b SMP memory/paging/TLB matrix. This is verification
only: no kernel, vmem, S159 fix path or S85b harness source was changed. The evidence branch is
`mark/K379`, based on `origin/master` `b8169cce429177dc82f207e18b695dbcf204437d`.

## Host and build

- Date: 2026-11-22 (UTC).
- Host: 20-CPU shared Linux host; `uptime` load average was **102.06** before the matrix,
  **93.30–122.47** during the KVM runs. The high load is recorded because S159 observed
  load-sensitive timing; it did not produce a failure here.
- QEMU: 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31).
- Kernel image: `build/smp-memory/smp-memory.img`, SHA-256
  `c62d2ba5821a9549aae792b404c6f6de38877bf19373fc989f4c668da305b052`.
- Kernel ELF: `build/x86_64/kernel-debug.elf`, SHA-256
  `e753c50f4f4374959018cf05a6798feb948d976d32315565501ab409964e2a84`.
- Build command: `run.sh` builds `tests/smp/memory` and runs
  `cargo xtask image --arch x86_64 --profile base --cmdline 'console=ttyS0 test-exit=1 smp_user=on smp-user-trace'`
  before the iterations.

## Matrix

The ticket omitted the iteration count. `N=10` per mode was chosen from the measured
single-iteration smoke times (TCG 22.677 s, KVM2 22.009 s, KVM4 15.275 s); ten iterations fit a
single proportional cap and give a dated baseline. The separate 50/50 release campaign is not
claimed here.

| Mode | QEMU SMP | Command | Exit | Runs | TLB timeouts | `tlb_stale` | Memory result | Duration worst / p90 / avg |
|---|---:|---|---:|---:|---:|---|---|---|
| TCG | 4 | `timeout 420 sh tests/smp/memory/run.sh tcg --iterations 10` | 0 | 10/10 | **0** | 0 every run | `smp-memory: ok` every run | 41,485 / 38,267 / 29,772 ms |
| KVM | 2 | `timeout 500 sh tests/smp/memory/run.sh kvm2 --iterations 10` | 0 | 10/10 | **0** | 0 every run | `smp-memory: ok` every run | 40,095 / 30,073 / 25,472 ms |
| KVM | 4 | `timeout 400 sh tests/smp/memory/run.sh kvm4 --iterations 10` | 0 | 10/10 | **0** | 0 every run | `smp-memory: ok` every run | 48,184 / 31,922 / 24,643 ms |

The harness modes are `kvm2` and `kvm4`, not the ticket's shorthand `kvm`; the 2-vCPU and
4-vCPU runs are recorded separately. The per-iteration data is in `matrix.tsv`.

## TLB vs memory separation

- Every run's serial log was scanned for the literal `smp: tlb shootdown timeout`; the count is
  **0 for all 30 runs**.
- Every run's summary line contains `tlb_stale=0`.
- Every run contains `smp-memory: ok` and the stress summary
  `alloc_ops=256 fault_pages=512 maps=32 forks=4 tlb_rounds=64 tlb_stale=0`.
- No memory/paging failure, no timeout line, and no retry. The run.sh stops on the first
  forbidden line; none occurred.

## Verdict

**PASS** for the dated 10/10 matrix on TCG `-smp 4`, KVM `-smp 2` and KVM `-smp 4` at
`b8169cce...`. TLB timeouts are counted separately and are zero; the memory/paging stress is
clean. This is a baseline, not the 50/50 release campaign.

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
