<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K365 TCP soak execution evidence

Base `origin/master` `60b2bf51`, branch `stuart/K365`. Cases are the merged K351 files; they were
not edited.

## Rows

| Case | Cap | Exit | Result | duration_ms | host load start -> end | serial log |
|---|---:|---:|---|---:|---|---|
| `tcp-soak-kvm2` | 700 s | 0 | PASS | 271744 | 2.99 -> 2.47 | `tcp-soak-kvm2.log` |
| `tcp-soak-kvm4` | 700 s | 0 | PASS | 255416 | 2.31 -> 2.55 | `tcp-soak-kvm4.log` |
| `tcp-soak-tcg4` | 1000 s | 0 | PASS | 260023 | 2.34 -> 1.92 | `tcp-soak-tcg4.log` |

`/dev/kvm` check: present. QEMU `exit_status` 33 is the expected `isa-debug-exit` success status.

## Commands

```sh
timeout 30 test -c /dev/kvm
timeout 700 cargo xtask qtest --case tcp-soak-kvm2 --arch x86_64
timeout 700 cargo xtask qtest --case tcp-soak-kvm4 --arch x86_64
timeout 1000 cargo xtask qtest --case tcp-soak-tcg4 --arch x86_64
```

Each command ran in the foreground; one row per shape, no retry.

## Workload and stall detection

Every case matched:

```text
smp: 2 cpus online / smp: 4 cpus online
tcp-ready
tcp-cycle: 1 .. tcp-cycle: 8
tcp-cycle-done: 1 .. tcp-cycle-done: 8
tcp-done
init exited (0)
```

The final heartbeat `tcp-cycle-done: 8` arrived within the case's stall budget; there are no
`stall`, `panic`, `EXCEPTION`, `tcp-echo: N` or `tcp-soak-workload:` lines. JSON `unmatched` and
`forbidden_hits` are empty for all three rows.

## Artifacts

`tcp-soak-<shape>.log` is the serial log, `.json` the qtest result, `.stdout.log` the foreground
console. `SHA256SUMS` covers every file here. Hashes:

```text
51309f4585e0345b9ad9f30c2bdfadbf4aaa1eeff61307c3cf6e2628ba871df7  tcp-soak-kvm2.log
0682239bcc0175dceab4accf33565cebaade38d326c4463573cab234adb6da93  tcp-soak-kvm4.log
0a26f20bde3eff285b12fcb5811fd052c7c685836a5594518c33d8cd3c9341fe  tcp-soak-tcg4.log
```
