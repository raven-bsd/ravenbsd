<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP memory matrix re-verification (K379)

Date: 2026-11-22. Tested tree: `origin/master`
`b8169cce429177dc82f207e18b695dbcf204437d`. This is a dated re-run of the merged S85b
`tests/smp/memory` matrix; no kernel, vmem, S159 fix path or harness source was changed.

## Matrix

The ticket did not name the iteration count. Ten iterations per mode was chosen from the measured
single-iteration smoke times (TCG 22.677 s, KVM2 22.009 s, KVM4 15.275 s), so each mode fits one
proportional cap. This is a baseline, not the 50/50 release campaign.

| Mode | QEMU SMP | Command | Exit | Runs | TLB timeouts | `tlb_stale` | Verdict |
|---|---:|---|---:|---:|---:|---|---|
| TCG | 4 | `timeout 420 sh tests/smp/memory/run.sh tcg --iterations 10` | 0 | 10/10 | 0 | 0 every run | PASS |
| KVM | 2 | `timeout 500 sh tests/smp/memory/run.sh kvm2 --iterations 10` | 0 | 10/10 | 0 | 0 every run | PASS |
| KVM | 4 | `timeout 400 sh tests/smp/memory/run.sh kvm4 --iterations 10` | 0 | 10/10 | 0 | 0 every run | PASS |

The harness has no `kvm` mode; `kvm2` and `kvm4` are the actual 2-vCPU and 4-vCPU runs. Every run
printed `smp-memory: summary alloc_ops=256 fault_pages=512 maps=32 forks=4 tlb_rounds=64
tlb_stale=0` and `smp-memory: ok`. No `smp: tlb shootdown timeout` line was present in any of the
30 serial logs, so no TLB timeout is folded into a generic pass.

## Duration and host

| Mode | Worst | p90 | Average |
|---|---:|---:|---:|
| TCG 4 | 41,485 ms | 38,267 ms | 29,772 ms |
| KVM 2 | 40,095 ms | 30,073 ms | 25,472 ms |
| KVM 4 | 48,184 ms | 31,922 ms | 24,643 ms |

Host load average was 102.06 before the matrix and 93.30–122.47 during the KVM runs. The high load
is recorded because S159 observed load-sensitive timing; it did not produce a failure. No cap was
raised to hide a timeout and no failing run was retried.

## Artifacts

Evidence is under
`tests/evidence/smp-memory/b8169cce429177dc82f207e18b695dbcf204437d/`: `README.md`,
`matrix.tsv`, the three `summary.txt` files, 30 `run-N.log` serial logs, 30 `run-N.json` qtest
results, three `command.log` files, `image-sha256.txt` and `SHA256SUMS`.

Image: `build/smp-memory/smp-memory.img`, SHA-256
`c62d2ba5821a9549aae792b404c6f6de38877bf19373fc989f4c668da305b052`. Kernel ELF:
`build/x86_64/kernel-debug.elf`, SHA-256
`e753c50f4f4374959018cf05a6798feb948d976d32315565501ab409964e2a84`.

## Verdict

**PASS** for TCG `-smp 4`, KVM `-smp 2` and KVM `-smp 4` on `b8169cce...`: 30/30 iterations
pass, TLB timeouts are zero, `tlb_stale=0`, and the user memory/paging stress is clean. This does
not replace the release's separate 50/50 campaign.
