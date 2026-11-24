# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# TCP soak qtest matrix (K351)

K351 adds the harness-driven half of the TCP soak matrix: one qtest case per
accelerator/SMP level. Each case boots the K285 `tcp-soak` image profile, logs in on
the serial console and runs the bounded smoke workload
(`/bin/brush /etc/tcp-soak-workload.sh 8 5 7000`: 8 connect/send/recv/close cycles, 5 s
between cycles, listener port base 7000 rotating over the next 50 ports).

## Cases

| Case | Accelerator | CPUs | Case timeout | Role |
| --- | --- | --- | --- | --- |
| `tests/qtest/cases/tcp-soak-kvm2.toml` | KVM (`-enable-kvm -cpu host`) | 2 | 600 s | matrix point |
| `tests/qtest/cases/tcp-soak-kvm4.toml` | KVM (`-enable-kvm -cpu host`) | 4 | 600 s | ticket §3 acceptance |
| `tests/qtest/cases/tcp-soak-tcg4.toml` | TCG (harness default) | 4 | 900 s | matrix point |

All cases run on the fixed harness machine (q35, 512 MiB, serial stdio, OVMF) and
append their accelerator/CPU contract to `qemu_extra`; the appended `-smp N` overrides
the harness's fixed `-smp 4` because the last option wins (the K284 S112 pattern), so
the CPU count is part of the case contract, as in K284.

## Derivation

* From K284 (`tests/qtest/cases/tcp-smp{2,4,4-tcg}.toml`, `docs/tcp-smp.md`): the
  accelerator/SMP matrix shape, the `qemu_extra` override pattern,
  `expect_exit = "success"` with `test-exit=1` in the case `boot_cmdline`, and the
  `^smp: N cpus online$` boot proof.
* From K285 (`profiles/tcp-soak.toml` + `profiles/tcp-soak.d/`,
  `scripts/tcp-soak-driver.py`, `docs/tcp-soak.md`): the `tcp-soak` image profile
  (release wiring, getty/login, `raven-nc` at `/sbin/nc`, the guest workload at
  `/etc/tcp-soak-workload.sh`), the marker protocol (`tcp-ready`, `tcp-cycle: N`,
  `tcp-echo: <rc>`, `tcp-wait: <rc>`, `tcp-cycle-done: N`, `tcp-done`) and the stall
  semantics.

The cases reuse the merged profile and workload; no guest code, fixture, profile or
driver was added or modified (the ticket's Owns-globs are the case files and this
document).

## Stall semantics in the harness

The K285 host driver fails a run on a missing `tcp-cycle-done` heartbeat
(`--stall`, default `max(30, interval*4)` seconds). In the harness the same condition
is expressed as a bounded `[[step]] wait` on the final heartbeat:
`wait = '^tcp-cycle-done: 8$'` with `timeout_secs = 300` (KVM cases) or `700`
(TCG case). A hang anywhere in the workload delays or removes the final heartbeat,
the wait deadline trips and the case fails with `no matching serial line within N s`.
A stall fails; the harness never retries. The preserved serial log
(`build/qtest/<case>.log`) is the stall evidence, and the result JSON
(`build/qtest/<case>.json`) records the outcome, duration and exit status.

A non-zero `nc` client exit code fails the case through the forbid table
(`^tcp-echo: [1-9]`), matching the K285 protocol ("non-zero fails the run immediately").
The workload's own argument-validation lines (`tcp-soak-workload: ...`) are likewise
forbidden, so a malformed workload invocation cannot pass silently.

## Running

Ticket §3 spells the acceptance command as `cargo xtask qtest run --case ...`. The
xtask forwarder (`xtask/src/qtest.rs`) already translates its own `--case` spelling to
the harness's `run --case <file>` subcommand and rejects a `run` positional
(`qtest: unexpected argument \`run\``), so the working spelling is:

```sh
timeout 900 cargo xtask qtest --case tests/qtest/cases/tcp-soak-kvm4.toml   # ticket §3 acceptance
timeout 900 cargo xtask qtest --case tests/qtest/cases/tcp-soak-kvm2.toml   # matrix
timeout 1200 cargo xtask qtest --case tests/qtest/cases/tcp-soak-tcg4.toml  # matrix (TCG budget)
```

Each run rebuilds the `tcp-soak` image for the case (the harness keeps no image
cache); the image carries the case's `boot_cmdline` in the ESP `cmdline.txt`. The
profile sets `[image] release = true`, so the image embeds the release kernel built
from the clean checkout at HEAD and the tree must be clean when the image builds.

## Bounded smoke vs longer owner runs

The qtest cases are the bounded smoke (8 cycles / 5 s interval): fast enough for a
CI-style gate while still exercising every socket state transition 8 times. The
longer evidence path stays the K285 host driver, which owns login, per-cycle
timestamps, the socket-state evidence stream and the JSONL event log:

```sh
timeout 900 make tcp-soak-smoke      # bounded 600 s workload, 10 s interval
timeout 90000 make tcp-soak-24h      # owner-only 24 h run (86400 s workload)
```

See `docs/tcp-soak.md` for the driver's protocol and artifact layout.

## Donor reconciliation

Test-only ticket: the cases exercise the merged K266/K271/K284 socket path already on
master. No donor code was copied.
