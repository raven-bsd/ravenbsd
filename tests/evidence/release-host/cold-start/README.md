<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Cold-start sample (S116b, 2026-11-21)

Five release-image boots on KVM4 at product commit `b04e71ad8`, measuring the dynamic
cold-start steps required by the release packet: `login:` -> `Password:` (`login_ms`) and
`Password:` -> `raven#` (`password_ms`). This is the per-step sample the packet's `cold-start`
row names; the full-boot kernel-entry -> login gate is S116's separate 10/10 direct-boot row.

## Command

```sh
timeout 900 sh scripts/login-cold-start.sh \
    --accel kvm --smp 4 --samples 5 --check \
    --out /tmp/opencode/s116-cold-start
```

- Product commit: `b04e71ad8`
- Release image: `build/login-cold-start/release.img`
- Image SHA-256: `a993cd2c37d3b63388bdbc3caf8137da4e0b98c2633fed60bb25df8b91752023`
- Host: 20 CPUs, `/dev/kvm`, QEMU 4-CPU, load1 20.55-23.93 during the samples
- Result: `check: PASS` (all medians and p90s <= 1000 ms)

## Samples

| Sample | Accel | vCPU | Commit | load1 | login_ms | password_ms | Error |
|---|---|---|---|---|---|---|---|
| 1 | kvm | 4 | `b04e71ad` | 23.93 | 57.2 | 105.8 | none |
| 2 | kvm | 4 | `b04e71ad` | 22.66 | 58.6 | 112.5 | none |
| 3 | kvm | 4 | `b04e71ad` | 22.66 | 61.1 | 112.9 | none |
| 4 | kvm | 4 | `b04e71ad` | 21.64 | 57.1 | 91.7 | none |
| 5 | kvm | 4 | `b04e71ad` | 20.55 | 56.3 | 93.6 | none |

| Step | n | min | median | p90 | max | 5/5 < 1000 ms |
|---|---|---|---|---|---|---|
| `login_ms` | 5 | 56.3 | 57.2 | 61.1 | 61.1 | yes |
| `password_ms` | 5 | 91.7 | 105.8 | 112.9 | 112.9 | yes |

## Artifacts

| File | Contents |
|---|---|
| `run.log` | Full command output: release image build at `b04e71ad8`, host line, five sample rows, table, summary and `check: PASS` |
| `kvm-smp4-sample1.log` .. `kvm-smp4-sample5.log` | Raw QEMU serial for each boot: kernel banner `b04e71ad8`, screen, `login:`, `Password:`, `raven#`, `exit 0`, `init exited (0)` |

The top-level `tests/evidence/release-host/README.md` row now points here; the generated
release-packet `cold-start` gate still reads `docs/releases/0.1.0-notes.md`, so its PENDING tag
is a packet-text update for the release-packet owner, not a missing sample.
