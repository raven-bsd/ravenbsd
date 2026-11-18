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
`--out DIR`, `--build`, `--wait-for-idle SEC`, `--max-load LOAD` (default `max(2, nproc/4)`; 5.0 on a 20-CPU host).

## Exclusive-host preflight

Both the soak and `scripts/kernel-gate.sh` call `tests/smp/boot/host-idle.sh` before starting any
QEMU. It refuses to run when any other `qemu-system-*`, `target/*/xtask`, `cargo xtask/build/test/
run` or `rustc` process is present, or when the 1-minute load average exceeds the bound (default
2.0). The refusal writes the exact host state (load, CPUs, QEMU/job list, KVM, QEMU version,
memory) to the run's `host-state.txt`; nothing is booted or retried.

On a shared host, pass `--wait-for-idle SECONDS` to the soak to poll in the foreground until the
host is idle. For the kernel gate, set `S107_WAIT_FOR_IDLE=SECONDS` (and optionally
`S107_MAX_LOAD`); the gate writes `target/kernel-gate/host-state.txt`. The preflight is a start
gate only: it cannot stop another lane from starting a QEMU during the run, so the result still
records the observed load and any failure.

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

The host was **not exclusive** during this campaign: other lane QEMU processes were running and
the load average was around 22. The kernel-gate ancillary run also exposed a real AP bring-up
failure under the same load: `irq-dist-off` logged `smp: cpu 3 did not start` and
`smp: 3 cpus online` (first gate run had a separate aarch64 `kernel-timer` 134 ms over the
7-130 ms expectation; a single retry of that timer case passed). These are reported as load-
sensitive SMP failures, not a clean boot-loop pass. A clean matrix must be rerun on an exclusive
host after the S83/AP issue is resolved.

## Rework round 2 preflight

The exclusive-host preflight was added and exercised. On the shared host it refused both gates
without booting anything:

- soak, `20261118T141132Z`: waited 900 s; load fell only from 84.56 to 22.80, with other
  `qemu-system`, xtask/cargo/rustc jobs still present; `host-idle: TIMEOUT`; no boot was run.
  State: `build/smp-boot-soak/20261118T141132Z/host-state.txt`.
- kernel gate: `host-idle: REFUSE load(27.86>2.0) qemu xtask/cargo/rustc`; exit 2; no case ran.
  State: `target/kernel-gate/host-state.txt`.

The preflight is doing its job: it prevents another contaminated matrix. The 50/50 KVM4,
20/20 KVM2, 20/20 KVM1 and 10/10 TCG4 matrix remains **not run** in round 2 because the host
never became idle; the round-1 first failure remains preserved and is not retried.

## Reproduction and ownership

The runner and helper are owned by S107 (`scripts/smp-boot-soak.sh`, `tests/smp/boot/**`,
`docs/smp-boot-soak.md`). The KVM AP fix is S83/Stuart; this ticket is evidence, not a kernel
change. The TLB/lock stress remains S85b/S85a/S102 and is not replaced by this boot-loop matrix.
