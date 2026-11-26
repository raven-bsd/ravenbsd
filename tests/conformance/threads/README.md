<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# S178 thread and futex conformance

Application-level thread/futex contract rows on the release image, run with user SMP on at four
CPUs under KVM and TCG. The probe prints the shared `CONFORM threads ...` row contract
(`tests/conformance/harness/conformance.rs`); the topic runner parses it with the shared harness.

| Row | Contract | Expected |
|---|---|---|
| futex-wait-wake | `FUTEX_WAIT` blocks, one `FUTEX_WAKE(1)` returns 1 | waiter returns 0, wake returns 1 |
| futex-eagain | `FUTEX_WAIT` with a changed word | `EAGAIN` (11), no block |
| futex-wake-limit | three waiters, `FUTEX_WAKE(1)` | first nonzero wake is exactly 1; exactly one waiter returns 0 |
| futex-requeue | `FUTEX_REQUEUE` moves the waiters to the target word | requeue 0, source wake 0, target wake 2 |
| robust-owner-died | owner dies holding a robust futex | word `FUTEX_OWNER_DIED` (0x40000000), one waiter woken with 0 |
| tls-per-thread | x86-64 `%fs:0` per thread, stable across yields | three distinct bases, each stable |
| thread-exit-tid | `THREAD_CREATE_EX` tp/tid_out/clear_tid, `THREAD_EXIT` | tid written, clear word 0, no kernel TLS destructor, next tid differs |
| sched-getaffinity | libc `sched_getaffinity(0, ...)` with user SMP on | CPU mask popcount equals the configured `--cpus` |

Two controlled mutations encode a broken implementation's belief and must turn their rows red:
`wake-one-extra` (a count-1 wake is expected to wake two) and `tls-shared-base` (two threads are
expected to share one TLS base). The product tree is not changed.

## Build and run

```text
python3 tests/conformance/threads/run.py --accel tcg
python3 tests/conformance/threads/run.py --accel kvm
python3 tests/conformance/threads/run.py --accel kvm --phase mutation --mutation wake-one-extra
```

The service passes `--cpus 4`, matching the four-CPU QEMU image the qtest harness boots. The
serial log and row verdict are written under `build/conformance/threads/`.
