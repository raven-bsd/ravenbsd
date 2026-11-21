# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# TCP soak (K285)

The TCP soak drives sustained TCP traffic through the merged FreeBSD-derived TCP path
(K266 input/ACK/window/options, K271 socket lifecycle, K276 syncache/SYN-flood, K288
TIME_WAIT/TLP/delayed-ACK) and reports stalls with preserved evidence. It complements the
base-system 24 h soak (`scripts/soak.sh`), which exercises the OS but not sustained TCP
traffic.

## Components

* `profiles/tcp-soak.toml` plus `profiles/tcp-soak.d/` — the `tcp-soak` image profile: the
  release image wiring (shell services, the QEMU user-network stack on 10.0.2.15/24,
  getty/login, release rc policy) plus `raven-nc` at `/sbin/nc` and the guest workload
  script at `/etc/tcp-soak-workload.sh`. The profile carries `[image] release = true`, so
  the image must embed the kernel built from the clean checkout at HEAD.
* `profiles/tcp-soak.d/tcp-soak-workload.sh` — the guest workload (brush). Each cycle:
  start one `nc -l -p <port> -e` echo listener in the background, run one client
  connect/send/recv/close round trip (14-byte payload) against `127.0.0.1`, reap the
  listener, snapshot the socket table with `/sbin/netstat`, print unique markers.
* `scripts/tcp-soak-driver.py` — the host supervisor. Boots QEMU (q35, 512 MiB, 4 vCPU,
  KVM when available, OVMF, user-mode NIC), logs in, starts the workload, timestamps
  every cycle and fails on a workload error, a boot timeout, a QEMU exit or a missing
  `tcp-cycle-done` heartbeat (stall).
* `scripts/tcp-soak.sh` — the entry point used by the Makefile targets.

## Workload protocol

The guest prints one line per marker; the host timestamps the raw serial stream.

| Line | Meaning |
| --- | --- |
| `tcp-ready` | workload started, about to run `<cycles>` cycles |
| `tcp-cycle: N` | cycle N begins (fresh listener + client round trip) |
| `tcp-echo: <rc>` | `nc` client exit code; non-zero fails the run immediately |
| `tcp-wait: <rc>` | listener reap status (informational) |
| `tcp-cycle-done: N` | cycle N complete and verified (the stall heartbeat) |
| `tcp-done` | all cycles finished |

Any other guest output between markers (the `nc` diagnostics and the per-cycle `netstat`
snapshot) is preserved as socket-state evidence.

## Stall semantics

A stall is no `tcp-cycle-done` for `--stall` seconds (default `max(30, interval*4)`).
On a stall the driver is a **failure, never a retry**: it writes
`<run>/stall-evidence.txt` containing the failure context, the last socket-state
evidence lines and the last 200 stamped serial lines, records a `stall` event in
`tcp-soak.jsonl`, and exits non-zero.

## Running

```sh
timeout 900 make tcp-soak-smoke        # bounded 10-minute smoke (default)
timeout 90000 make tcp-soak-24h        # owner-only 24 h run (86400 s workload)
sh scripts/tcp-soak.sh --check         # fast proof: tools + firmware + injected-stall selftest, no QEMU
python3 scripts/tcp-soak-driver.py --selftest   # the driver-level selftest alone
```

First run builds the `tcp-soak` image from the clean checkout at HEAD (the slow path);
later runs reuse `build/tcp-soak/image.img` unless `--build` is given or the tree is
dirty at build time.

Run artifacts live under `build/tcp-soak/run-<UTC>/`:

* `serial.log` / `serial.stamped.log` — raw and timestamped serial stream
* `tcp-soak.jsonl` — event stream (start, cycle, echo, wait, cycle_done, stall, failure)
* `socket-state.log` — per-cycle socket/traffic evidence (netstat snapshots, `nc` lines)
* `stall-evidence.txt` — failure-time evidence dump (only on non-pass)
* `summary.json` / `summary.txt` — machine- and human-readable summary

## Evidence gap: pcap

In-guest pcap capture is not part of the 0.1.0 soak: the `tcp-soak` profile does not
package `raven-tcpdump` (the K281 port) yet, so the evidence dump records
`pcap: n/a`. Backlog: package `raven-tcpdump` in a `tcp-soak` profile variant and let
the workload record the loopback flow while the driver preserves the capture next to
the serial evidence. Zero-window/retransmit events at the 14-byte loopback payload are
not expected to trigger; the per-cycle `netstat` snapshots expose the TCP states and
interface counters, and a higher-volume payload mode is a backlog item.
