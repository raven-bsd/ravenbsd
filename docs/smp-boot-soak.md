<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# KVM/TCG release-image boot-loop soak (S107)

This is the independent S83 SMP boot-loop gate. It is separate from the S85b TLB memory/lock
stress: one proves that every CPU count reaches the release login repeatedly, the other proves the
memory/TLB paths under stress.

## Matrix

| Mode | Count | QEMU |
|---|---:|---|
| KVM `-smp 4` | 50 | `-enable-kvm -cpu host -smp 4` |
| KVM `-smp 2` | 20 | `-enable-kvm -cpu host -smp 2` |
| KVM `-smp 1` | 20 | `-enable-kvm -cpu host -smp 1` |
| TCG `-smp 4` | 10 | `-smp 4` (no KVM) |

Each boot uses the x86_64 release image, the OVMF firmware pair and the release-login virtio-net
device from `scripts/qemu-kvm.sh`. KVM aarch64 is not available on this host; the matrix is
x86_64 only.

## Build and run

The release image builder requires a clean worktree. Build it once on the commit under test:

```sh
cargo xtask image --arch x86_64 --profile release
```

Then run the foreground soak. The runner uses the existing image by default; `--build` forces a
rebuild (and will fail on a dirty tree by design):

```sh
sh scripts/smp-boot-soak.sh --kvm4 50 --kvm2 20 --kvm1 20 --tcg4 10
```

Useful options: `--timeout-kvm SEC` (default 15), `--timeout-tcg SEC` (default 30), `--image PATH`,
`--out DIR`, `--build`.

## Timing and pass criteria

`tests/smp/boot/soak_boot.py` supervises each QEMU process through a pty, timestamps the serial
lines as they arrive and terminates QEMU immediately when the first `login:` prompt appears (or at
the timeout). The shell runner records `login_s = first login timestamp - QEMU start`.

A boot passes only when the serial log contains:

- `smp: N cpus online` for the requested N;
- a `login:` prompt;
- no `CPU Reset`, `triple fault` or `KERNEL PANIC` marker.

The runner stops on the first failure, preserves the raw/stamped log, and does not retry it. A
timeout without login is a failure, not a pass. The login prompt is never synthesized.

Per-mode p50/p90/worst are written to `build/smp-boot-soak/<UTC>/summary.txt`; every boot keeps
`<mode>/run-N.log` and `run-N.stamped.log`. KVM and TCG are reported separately.

## Results

Campaign at `94648948` (release image built on the clean tree), run `20261118T133430Z`:

| Mode | Requested | Passed | Failure | p50 | p90 | worst |
|---|---:|---:|---|---:|---:|---:|
| KVM `-smp 1` | 20 | 20 | none | 3.890 s | 4.524 s | 5.072 s |
| KVM `-smp 2` | 20 | 20 | none | 4.264 s | 4.573 s | 4.620 s |
| KVM `-smp 4` | 50 | 20 | **run 21/50 at timeout** | 4.815 s | 10.693 s | 13.651 s |
| TCG `-smp 4` | 10 | 0 | not run; stopped on KVM4 failure | — | — | — |

KVM4 run 21 produced only OVMF screen-clear escape sequences (29 bytes) and no `RavenBSD
kernel`, `smp: 4 cpus online` or `login:` line before the 15 s timeout. The runner stopped there
and did not retry it. The raw/stamped logs are preserved at
`build/smp-boot-soak/20261118T133430Z/kvm4/run-21.log` and `.stamped.log`; KVM1/KVM2 logs and
the failure's neighbors are in the same run root.

## Reproduction and ownership

The runner and helper are owned by S107 (`scripts/smp-boot-soak.sh`, `tests/smp/boot/**`,
`docs/smp-boot-soak.md`). The KVM AP fix is S83/Stuart; this ticket is evidence, not a kernel
change. The TLB/lock stress remains S85b/S85a/S102 and is not replaced by this boot-loop matrix.
