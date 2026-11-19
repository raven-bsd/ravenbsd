<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# 4 GiB / 4 vCPU boot and SMP-memory smoke (S157)

Status: **PASS (functional).** The 4 GiB release image boots on four vCPUs, reaches login and the
brush prompt, and completes `uname -a`, `ls /bin`, `ps --posix`, `du /tmp` and `exit 0` with
`init exited (0)`. The timing table below was recorded on a loaded shared host, so it is a
loaded-host sample, not an idle-host timing PASS.

## Qtest

- Case: `tests/qtest/cases/smp-4g.toml`, release profile, `-m 4096 -smp 4`, with the release
  virtio-net device.
- `timeout 300 cargo xtask qtest --case smp-4g --arch x86_64 --no-build` — **PASS**.
- Boot evidence: `smp: 4 cpus online`, `mem: 3991 MiB usable in 9 regions`.
- Command evidence: `uname -a` prints
  `RavenBSD raven 0.1 RavenBSD 0.1 (BASE) #1: 75aba662 2026-11-19 x86_64`; `ls /bin` prints the
  base tools; `ps --posix` prints `PID PPID UID GID CMD`; `du /tmp` prints `0 /tmp`; the guest
  exits cleanly.

The `smp: 4 cpus online` expectation is unordered because this kernel prints it before the
banner. The 4G image build measured **167.19 s** (cap 300).

## 5 KVM + 5 TCG matrix

`scripts/smp-4g.sh` runs five KVM and five TCG boots in the foreground, logs in, executes the
same four commands and exits, preserving one raw and one stamped serial log per run plus a host
table. `timeout 300 sh scripts/smp-4g.sh` — **PASS, 10/10** at
`build/smp-4g/20261119T153747Z/`.

| Mode | Run | login_s | password→prompt_s | load1 before/after |
|---|---:|---:|---:|---:|
| KVM | 1 | 3.929 | 0.041 | 5.63/5.18 |
| KVM | 2 | 3.535 | 0.044 | 5.18/5.00 |
| KVM | 3 | 3.601 | 0.042 | 5.00/4.84 |
| KVM | 4 | 3.568 | 0.068 | 4.84/4.85 |
| KVM | 5 | 3.864 | 0.041 | 4.85/4.54 |
| TCG | 1 | 7.105 | 1.124 | 4.54/4.32 |
| TCG | 2 | 9.482 | 1.946 | 4.32/4.40 |
| TCG | 3 | 7.089 | 1.077 | 4.40/3.78 |
| TCG | 4 | 7.982 | 1.673 | 3.78/3.33 |
| TCG | 5 | 7.887 | 1.039 | 3.33/3.56 |

The host load was 3.3-5.6 during the matrix. The samples are recorded as loaded; no idle-host
timing claim is made. There was no panic, OOM, missing CPU, wrong memory size or command hang;
the `du /tmp` command passed on all ten boots.

## Reproduction

```sh
timeout 300 cargo xtask image --arch x86_64 --profile release \
  --cmdline 'console=ttyS0 test-exit=1' --output build/qtest/smp-4g.img
timeout 300 cargo xtask qtest --case smp-4g --arch x86_64 --no-build
timeout 300 sh scripts/smp-4g.sh
timeout 150 make check-fast
```

The script uses a 20 s KVM / 30 s TCG login cap and a 30 s per-command cap, with the outer
`timeout 300` covering the ten-run matrix. It stops on the first failure and preserves the run.

## DR-52 donor reconciliation

This is a first-party test. No donor code was copied or translated; it drives the existing
RavenBSD `MAP`, initfs and console paths.
