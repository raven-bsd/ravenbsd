<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP memory matrix re-verification (K379/K398)

Two dated runs of the merged S85b `tests/smp/memory` matrix. K379 measured the pre-NMI tree
`b8169cce`; K398 re-measured the post-NMI tree `879d3d39`, whose history includes S159's NMI
shootdown delivery fix `9078d345`. Verification only: no kernel, vmem, S159 fix path or harness
source was changed by either ticket.

## K398 — post-NMI matrix (`879d3d39`)

Ten iterations per mode, as the K398 contract requires. The harness has `tcg`, `kvm2` and `kvm4`
modes; `kvm` is shorthand, so both KVM widths are recorded separately.

| Mode | QEMU SMP | Command | Exit | Runs | TLB timeouts | `tlb_stale` | Verdict |
|---|---:|---|---:|---:|---:|---|---|
| TCG | 4 | `timeout 600 sh tests/smp/memory/run.sh tcg --iterations 10` | 0 | 10/10 | 0 | 0 every run | PASS |
| KVM | 2 | `timeout 400 sh tests/smp/memory/run.sh kvm2 --iterations 10` | 0 | 10/10 | 0 | 0 every run | PASS |
| KVM | 4 | `timeout 400 sh tests/smp/memory/run.sh kvm4 --iterations 10` | 0 | 10/10 | 0 | 0 every run | PASS |

Every run printed `smp-memory: summary alloc_ops=256 fault_pages=512 maps=32 forks=4 tlb_rounds=64
tlb_stale=0` and `smp-memory: ok`. No `smp: tlb shootdown timeout` line was present in any of the
30 serial logs, so no TLB timeout is folded into a generic pass.

| Mode | Worst | p90 | Average |
|---|---:|---:|---:|
| TCG 4 | 12,916 ms | 11,856 ms | 8,181 ms |
| KVM 2 | 5,049 ms | 4,924 ms | 4,663 ms |
| KVM 4 | 10,792 ms | 7,719 ms | 5,731 ms |

Host load average was 23.65 before the matrix and 35.16 after it. QEMU 6.2.0. No cap was raised to
hide a timeout and no failing run was retried.

## K379 — pre-NMI baseline (`b8169cce`)

| Mode | QEMU SMP | Command | Exit | Runs | TLB timeouts | `tlb_stale` | Verdict |
|---|---:|---|---:|---:|---:|---|---|
| TCG | 4 | `timeout 420 sh tests/smp/memory/run.sh tcg --iterations 10` | 0 | 10/10 | 0 | 0 every run | PASS |
| KVM | 2 | `timeout 500 sh tests/smp/memory/run.sh kvm2 --iterations 10` | 0 | 10/10 | 0 | 0 every run | PASS |
| KVM | 4 | `timeout 400 sh tests/smp/memory/run.sh kvm4 --iterations 10` | 0 | 10/10 | 0 | 0 every run | PASS |

K379 durations: TCG worst 41,485 ms / average 29,772 ms; KVM2 40,095 / 25,472 ms; KVM4 48,184 /
24,643 ms. Host load was 102.06 before and 93.30–122.47 during the KVM runs.

## K379 vs K398

| Mode | K379 runs | K379 TLB | K379 worst / avg | K398 runs | K398 TLB | K398 worst / avg | Difference |
|---|---:|---:|---|---:|---:|---|---|
| TCG 4 | 10/10 | 0 | 41,485 / 29,772 ms | 10/10 | 0 | 12,916 / 8,181 ms | pass and TLB counts identical; K398 ~3.6x faster |
| KVM 2 | 10/10 | 0 | 40,095 / 25,472 ms | 10/10 | 0 | 5,049 / 4,663 ms | pass and TLB counts identical; K398 ~5.5x faster |
| KVM 4 | 10/10 | 0 | 48,184 / 24,643 ms | 10/10 | 0 | 10,792 / 5,731 ms | pass and TLB counts identical; K398 ~4.3x faster |

The duration difference tracks the host load difference (K379: 93–122; K398: 23–35) and is not
attributed to the NMI fix. The post-fix comparison is: no regression, no TLB timeout, no stale
line. The NMI-specific IF=0 delivery proof is S159's `selftest: smp tlb if0 ok elapsed_ms=1` and
its mutation, not this stress matrix.

## Artifacts

K398:
`tests/evidence/smp-memory/879d3d390ba4d344928d0d3183e780645329fb4e/` with `README.md`,
`matrix.tsv`, three `summary.txt` files, 30 `run-N.log` serial logs, 30 `run-N.json` results, three
`command.log` files, `image-sha256.txt` and `SHA256SUMS`. Image SHA-256
`bb50b1ee7eefaeb6e321a7816c9c55bc40321f3b9fb4278d86519e5c4f982b54`; kernel ELF
`e227b982f7d1304aff2860a509c927fb309c3fbf688df878eb619fb023f5bc1c`.

K379:
`tests/evidence/smp-memory/b8169cce429177dc82f207e18b695dbcf204437d/` with the same layout.
Image SHA-256 `c62d2ba5821a9549aae792b404c6f6de38877bf19373fc989f4c668da305b052`; kernel ELF
`e753c50f4f4374959018cf05a6798feb948d976d32315565501ab409964e2a84`.

Larger build artifacts stay in the product clone; their hashes are above.

## Verdict

**PASS** for both dated matrices. On the post-NMI tree `879d3d39`, TCG `-smp 4`, KVM `-smp 2` and
KVM `-smp 4` each pass 10/10 with zero TLB timeouts and `tlb_stale=0`; the result matches K379's
pre-NMI baseline with no regression. This does not replace the release's separate 50/50 campaign.
