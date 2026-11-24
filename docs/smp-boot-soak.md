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
QEMU. It refuses to run when another `qemu-system-*`, `xtask`, `cargo`, `rustc`, `clippy-driver`
or `rustdoc` process is present, or when the 1-minute load average exceeds the bound (default
`max(2, nproc/4)`; 5.0 on this 20-CPU host). Processes are matched by executable name (`ps comm`),
not by the full command line, so a shell that merely mentions `qemu`/`cargo` does not block the
gate; zombie, dead, stopped and traced build jobs are ignored, while a stopped QEMU is still
counted because it retains KVM and image resources. The refusal writes the exact host state
(load, CPUs, QEMU/job list, KVM, QEMU version, memory) to the run's `host-state.txt`; nothing is
booted or retried.

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

## Per-run host telemetry

Every boot writes one row to `host.tsv` in the campaign directory. The columns are `mode`, `run`,
`start_utc`, `end_utc`, `load1_before`, `qemu_before`, `jobs_before`, `load1_after`,
`qemu_after`, `jobs_after`, `status` and `login_s`. The before sample is taken immediately
before the QEMU helper starts; the after sample immediately after it returns. `qemu` counts live
`qemu-system-*` processes; `jobs` counts live `xtask`/`cargo`/`rustc`/`clippy-driver`/`rustdoc`
processes, ignoring zombie, dead, stopped and traced processes (the same class as the preflight).
A failed boot is still recorded with `status=fail` and an empty `login_s` before the runner
exits. `boots` in the summary line is the number of recorded rows (attempts), including a failed
boot.

The samples are best-effort and never change the pass/fail decision, timeout or login detection.
At exit the runner prints one summary line:

```text
smp-boot-soak: host telemetry boots=N max_load1_before=... max_load1_after=... max_qemu_before=... max_qemu_after=... max_jobs_before=... max_jobs_after=... file=...
```

The artifact is not a substitute for `host-state.txt`: the latter is the single start-gate
snapshot, while `host.tsv` is the per-boot time series.

## Round 1 results

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

## Round 3 matrix (2026-11-18, historical)

The exclusive-window rerun was taken after the S113 merge (`2972e15b` kernel; the release image
was built there) and the gate fix `f00328ea` (executable-name matching). The preflight at
19:54:21Z recorded load1 4.26 on 20 CPUs, `max_load=5`, no other QEMU and no build job, then the
runner executed the full matrix in one foreground campaign:

| Mode | Requested | Passed | Failure | p50 | p90 | worst |
|---|---:|---:|---|---:|---:|---:|
| KVM `-smp 1` | 20 | 20 | none | 4.415 s | 8.088 s | 12.653 s |
| KVM `-smp 2` | 20 | 20 | none | 4.634 s | 5.535 s | 8.551 s |
| KVM `-smp 4` | 50 | 50 | none | 4.548 s | 6.256 s | 10.714 s |
| TCG `-smp 4` | 10 | 10 | none | 9.617 s | 11.466 s | 12.387 s |

Campaign: `build/smp-boot-soak/20261118T195421Z/`; state:
`build/smp-boot-soak/20261118T195421Z/host-state.txt`; summary: `summary.txt`.
KVM4 run 21 **passed** with `login_s=4.208`, `smp: 4 cpus online` and `login:`; the earlier
29-byte run-21 timeout in `20261118T133430Z` is preserved and was not retried or overwritten.
All requested boots reached `smp: N cpus online` and login with no reset/panic marker. These are
observed login times on a shared 20-CPU host, not a <1 s boot-time claim (S116 owns that).

The independent lead integration run in `20261118T194209Z` (13:44 image) also completed
20/20, 20/20, 50/50 and 10/10; the round-3 campaign above is the branch-local evidence.

## Round 5 matrix (2026-11-18)

The rev-47 rerun used the relaxed shared-host bound while still refusing live QEMU/build jobs:

```sh
sh scripts/smp-boot-soak.sh --kvm4 50 --kvm2 20 --kvm1 20 --tcg4 10 --wait-for-idle 1800 --max-load 15
```

The preflight waited from load1 16.76 at 20:12:37Z until load1 3.07 at 20:37:52Z, then
recorded no other QEMU and no xtask/cargo/rustc job. KVM was clean:

| Mode | Requested | Passed | Failure | p50 | p90 | worst |
|---|---:|---:|---|---:|---:|---:|
| KVM `-smp 1` | 20 | 20 | none | 3.402 s | 3.753 s | 3.935 s |
| KVM `-smp 2` | 20 | 20 | none | 3.429 s | 3.906 s | 3.950 s |
| KVM `-smp 4` | 50 | 50 | none | 3.512 s | 3.817 s | 3.977 s |
| TCG `-smp 4` | 10 | 4 | **run 5/10 login timeout** | 8.530 s* | 10.262 s* | 10.262 s* |

`*` partial over the four passing TCG4 runs; the fifth failed, so the runner stopped and did not
run 6-10. KVM4 run 21 passed with `login_s=3.817`; the old round-1 run-21 failure remains
preserved. Campaign: `build/smp-boot-soak/20261118T201237Z/`.

TCG4 run 5 reached `smp: 4 cpus online` and `svcd: all services up` at 22.03 s, but the
`RavenBSD 0.1 (x86_64) (tty:console)` banner and `login:` prompt never appeared before the
30 s timeout; the log ends at 22.03 s (3,773 bytes). Passing TCG4 runs printed the banner at
about 8-10 s. Raw/stamped logs:
`build/smp-boot-soak/20261118T201237Z/tcg4/run-5.log` and `.stamped.log`. This is a real
TCG4 boot-to-login failure, preserved and not retried; it needs the TCG/getty owner. The earlier
round-3 TCG4 10/10 is **historical**; the latest campaign is TCG4 **4/10**. The clean
exclusive-host rerun belongs to **S126-r1 (Otto)**, using this instrumented runner; S107 does
not retry the failure away.

## Reproduction and ownership

The runner, helper and telemetry are owned by S107 (`scripts/smp-boot-soak.sh`,
`tests/smp/boot/**`, `docs/smp-boot-soak.md`). The KVM AP fix is S83/Stuart; this ticket is
evidence, not a kernel change. The TLB/lock stress remains S85b/S85a/S102 and is not replaced by
this boot-loop matrix.

The round-5 TCG4 failure is a TCG/getty boot-to-login issue, not a reason to retry the matrix
in S107. The clean exclusive-host rerun is **S126-r1 (Otto)**, using this instrumented runner
and its per-boot `host.tsv`.
