<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP soak run (KVM/TCG)

Status: K365, 2026-11-21. Evidence tags: **OBSERVED**, **PROPOSED**, **UNVERIFIED**.

This page records the first execution of the merged K351 TCP soak cases on merged RACK master. The
case files and the soak workload were not edited.

## 1. Matrix

* Base `origin/master` `60b2bf51`.
* `/dev/kvm` present; host load 1.92-2.99 during the rows.
* Each case is the K351 release `tcp-soak` profile: login as root, run the bounded 8-cycle
  workload (5 s interval, port base 7000), then exit through the test-exit path.

| Case | Cap | Exit | Result | duration | host load start -> end |
|---|---:|---:|---|---:|---|
| `tcp-soak-kvm2` | 700 s | 0 | PASS | 271.744 s | 2.99 -> 2.47 |
| `tcp-soak-kvm4` | 700 s | 0 | PASS | 255.416 s | 2.31 -> 2.55 |
| `tcp-soak-tcg4` | 1000 s | 0 | PASS | 260.023 s | 2.34 -> 1.92 |

QEMU `exit_status` 33 is the expected `isa-debug-exit` success status for `test-exit=1`.

## 2. Commands

```sh
timeout 30 test -c /dev/kvm
timeout 700 cargo xtask qtest --case tcp-soak-kvm2 --arch x86_64
timeout 700 cargo xtask qtest --case tcp-soak-kvm4 --arch x86_64
timeout 1000 cargo xtask qtest --case tcp-soak-tcg4 --arch x86_64
```

All ran in the foreground. One row per shape; no retry.

## 3. Cycle and stall evidence

Every row matched the CPU line, the ready marker, cycles 1-8, every `tcp-cycle-done`, `tcp-done`
and `init exited (0)`. The stall detector is the final `tcp-cycle-done: 8` deadline (300 s KVM,
700 s TCG); it arrived in every row.

```text
smp: 2 cpus online        tcp-soak-kvm2
smp: 4 cpus online        tcp-soak-kvm4, tcp-soak-tcg4
tcp-ready
tcp-cycle: 1 .. 8
tcp-cycle-done: 1 .. 8
tcp-done
init exited (0)
```

No `stall`, `panic`, `EXCEPTION`, `tcp-echo: N` or `tcp-soak-workload:` line occurred. The qtest
JSON has `unmatched: []` and `forbidden_hits: []` for all three rows.

## 4. Artifacts

`tests/evidence/tcp/soak/` holds each serial log, qtest JSON and foreground stdout, plus
`SHA256SUMS`:

```text
51309f4585e0345b9ad9f30c2bdfadbf4aaa1eeff61307c3cf6e2628ba871df7  tcp-soak-kvm2.log
0682239bcc0175dceab4accf33565cebaade38d326c4463573cab234adb6da93  tcp-soak-kvm4.log
0a26f20bde3eff285b12fcb5811fd052c7c685836a5594518c33d8cd3c9341fe  tcp-soak-tcg4.log
```

## 5. Donor reconciliation

Test/evidence only. The cases exercise the FreeBSD TCP core ported under K265-K278; no donor code
is copied by this ticket. The K351 case contract is reused unchanged.

## 6. Gaps

* This is the first execution of the K351 cases on merged master; the timings are observed
  loaded-host samples, not an idle performance claim.
* No failure occurred, so there is no blocked/regression finding to hand to a group owner.
* The `tcp-soak` profile and workload are not edited here; a future workload change needs its own
  case/test update.
