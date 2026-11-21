<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Cold-start boot sample (S116b, 2026-11-21)

Five release-image boots on KVM4 at the current merged sha. `kernel_ms` is the
`boot-time.sh` OS gate: the first `RavenBSD kernel` banner -> the getty `login:` prompt that
hands the session to brush. `firmware_ms` and `total_ms` are reported separately because OVMF
dominates the wall time; they are not part of the kernel gate.

## Command

```sh
timeout 1200 sh scripts/boot-time.sh --smp 4 --runs 5 --out /tmp/opencode/s116b-boot-time
```

- Measured product commit: **`b58c5ac97`** (current `origin/master`)
- Release image SHA-256: `24c42352f5c40f09b853e2f82c52a77acd218df7bfc7e9fedd6968ad14af966e`
- Host: 20 CPUs, `/dev/kvm`, KVM 4 vCPU; load1 **14.55** before, **23.97** after the run.
- Guest kernel id checked against HEAD on every run; `boot-time: GATE PASS: KVM 4-CPU
  kernel->login median 558ms < 1000ms`.

## Runs

| Run | firmware_ms | kernel_ms | total_ms | kernel_id |
|---|---|---|---|---|
| 1 | 3869 | 250 | 4119 | `b58c5ac97` |
| 2 | 6401 | 558 | 6959 | `b58c5ac97` |
| 3 | 6887 | 830 | 7717 | `b58c5ac97` |
| 4 | 7990 | 710 | 8701 | `b58c5ac97` |
| 5 | 4976 | 428 | 5404 | `b58c5ac97` |

| Phase | min | median | p90 | max | 5/5 < 1000 ms |
|---|---|---|---|---|---|
| `kernel_ms` | 250 | 558 | 830 | 830 | **yes** |
| `firmware_ms` | 3869 | 6401 | 7990 | 7990 | n/a (OVMF) |
| `total_ms` | 4119 | 6959 | 8701 | 8701 | no (firmware-dominated) |

The kernel boot gate is 5/5 under 1 s. The full QEMU/OVMF wall time is 4.1-8.7 s on this
shared host; that is reported honestly and is not the S116 direct-boot row. The separate
`tests/evidence/release-host/cold-start/` sample covers `login:` -> `Password:` and
`Password:` -> `raven#`.

## Artifacts

| File | Contents |
|---|---|
| `run.log` | full `boot-time.sh` output: image build at `b58c5ac97`, five rows, p50/p90/max, gate |
| `run-1.log` .. `run-5.log` | raw QEMU serial for each boot: loader, `RavenBSD kernel ... b58c5ac97`, services, screen, `login:` |
