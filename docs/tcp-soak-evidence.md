# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# TCP soak evidence runner (K300)

The evidence runner wraps the K285 TCP soak (`scripts/tcp-soak.sh` +
`scripts/tcp-soak-driver.py`) with two additions: a host-side stall monitor
for long runs and post-run artifact-hash evidence. It exists so the owner can
run one reproducible 24 h command whose output can be re-verified later.

## Components

* `scripts/tcp-soak-evidence.py` — the core: runner, collector, verifier and
  the no-QEMU `--selftest`.
* `scripts/tcp-soak-evidence.sh` — the POSIX entry point used by the commands
  below.

## Two levels of stall detection

| Level | Detects | Fires when | Behaviour |
| --- | --- | --- | --- |
| Driver (K285) | guest-level stall | no `tcp-cycle-done` heartbeat for `--stall` seconds (driver default `max(30, interval*4)`) | writes `stall-evidence.txt`, `result=stall` in `summary.json`, exits non-zero |
| Runner (K300) | host-side hang (dead serial, wedged script, QEMU with no output) | no byte growth in the run artifacts (`tcp-soak.jsonl`, `serial.stamped.log`, `serial.log`, `socket-state.log`) for `--stall` seconds (runner default 300) | writes `<out>/runner-stall-evidence.txt` (stream tails + sha256 manifest of everything present), kills the child process group, exits non-zero |

A stall is a **failure, never a retry**, at either level. The guest cannot
report a hang of its own reporting path, which is why the runner watches the
artifacts instead of trusting the guest.

## Evidence files

After the run (pass or fail) the collector writes into the run directory:

* `evidence.json` — machine-readable manifest, schema
  `ravenbsd.tcp-soak.evidence/1`: collection time, run directory, the full
  `summary.json`, and for every run artifact its name, size and sha256
  (`summary.json`, `summary.txt`, `serial.log`, `serial.stamped.log`,
  `tcp-soak.jsonl`, `socket-state.log`, and `stall-evidence.txt` when the
  driver recorded a failure). If `--image` was given and the file exists, its
  sha256 and size are recorded too.
* `evidence.txt` — the same manifest as `sha256  <path>` lines,
  sha256sum-compatible, so the owner can re-verify with coreutils:

  ```sh
  sha256sum -c build/tcp-soak/run-<UTC>/evidence.txt
  ```

`evidence.json`/`evidence.txt` themselves are not hashed (no self-reference).

## Running

```sh
sh scripts/tcp-soak-evidence.sh --check                 # fast proof, no QEMU
python3 scripts/tcp-soak-evidence.py --selftest         # collector selftest alone

# bounded smoke proof (the K300 ticket proof): 12 cycles through the runner
timeout 300 sh scripts/tcp-soak-evidence.sh --run --duration 120 \
    --interval 10 --stall 300 --out build/tcp-soak-evidence

# verify a finished run's evidence again later
sha256sum -c build/tcp-soak/run-<UTC>/evidence.txt
sh scripts/tcp-soak-evidence.sh --verify build/tcp-soak/run-<UTC>/evidence.json

# collect evidence for an existing run directory (e.g. from make tcp-soak-smoke)
sh scripts/tcp-soak-evidence.sh --collect build/tcp-soak/run-<UTC> --image build/tcp-soak/image.img
```

## The 24 h command (owner-only, not claimed by K300)

```sh
timeout 90000 sh scripts/tcp-soak-evidence.sh --run --duration 86400 \
    --interval 30 --stall 300
```

86400 s workload + 3600 s margin. With `--interval 30` the natural per-cycle
silence is ~33 s, well under the 300 s runner threshold; the driver-level
guest stall threshold stays at its default (`max(30, interval*4) = 120 s`).
The 24 h run reuses `build/tcp-soak/image.img` unless it is missing, in which
case `scripts/tcp-soak.sh` builds it from the clean checkout at HEAD.

## Exit codes (`--run`)

* `0` — the soak child passed and evidence was collected.
* the child's non-zero code — the soak failed (workload error, driver stall,
  boot timeout); evidence is still collected before exit.
* `3` — runner-level stall (the runner killed the child tree).
* `4` — runner usage/environment error (e.g. no run directory appeared).

## Operational note: host CPU load stretches guest time

The soak guest runs 4 KVM vCPUs that share host CPU time with everything else
on the machine. When the host is oversubscribed, every guest wall-clock
measurement — including the workload's `sleep`s — stretches in host time.
Measured (K300, 2026-11-21, 20-core host): with the 15-minute load average at
~69 (3.5 x oversubscription, covering the run window 03:53-03:58Z) the
workload's 10 s interval cycles ran at a steady 33.2 s host period (a uniform
~3.0 x guest-time stretch), and the driver's host deadline failed the run at
cycle 8 of 12; on the same image with the 1-minute load average dropping from
~28 to ~16 the same run completed 12/12 cycles at an 11.1 s host period in
150 s. The image bytes were identical in both runs. Run the 24 h soak
on a quiet host, or expect the workload to finish in proportionally more
wall time (and to hit the driver deadline early under heavy contention).
The runner's 300 s stall threshold sits above the stretched-cycle silence
(~33 s), so load alone does not trip the runner-level stall detector; it
does trip the driver-level deadline, which is the intended failure.

## Evidence gap (inherited from K285)

In-guest pcap capture is not part of 0.1.0: the tcp-soak profile does not
package `raven-tcpdump` (the K281 port) yet, so the evidence covers the serial
stream, the event stream and the per-cycle netstat snapshots. See
`docs/tcp-soak.md` for the pcap backlog item.
