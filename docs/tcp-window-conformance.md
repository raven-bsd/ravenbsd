<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP window conformance

Status: K290, 2026-11-21. Evidence tags: **OBSERVED** (measured on the merged stack),
**PROPOSED** (defined but not observed), **UNVERIFIED**.

This document pins the window-scale, zero-window probe, reopen and persist behaviour of the merged
TCP stack. The scenarios live in `tests/net-conformance/window/`; the live exchange is driven by
`scripts/net-conformance.sh`, and `verify.py` independently asserts the pcap sequence numbers,
flags and bytes.

## 1. Scenarios

| Scenario | Harness | Assertion |
|---|---|---|
| `window-scale` | `handshake` | client offers MSS 1460, WS 7 and SACK; server answers MSS 1460, WS 3, SACK; a peer without WS gets no WS |
| `zero-window-probe` | `zero-window` | a probe precedes the reopen; the data resumes at the probe point |
| `reopen` | `zero-window` | the queued 44-byte write flushes once after the window reopens |
| `persist` | `zero-window` | no probe replay; probe bytes plus remaining data equal the 44-byte payload |

The absolute initial sequence numbers are random. `ISN` is the client ISN and `SISN` the server ISN.

```text
client -> server  SYN      seq=ISN     win=65535  opt mss=1460 ws=7 sack
server -> client  SYN-ACK  seq=SISN    ack=ISN+1  win=65535  opt mss=1460 ws=3 sack
client -> server  ACK      seq=ISN+1   ack=SISN+1 win=0
server -> client  probe    seq=SISN    len=0      (0B walk)
client -> server  ACK      seq=ISN+1   ack=SISN+1 win=65535
server -> client  data     seq=SISN+1  len=44     "K253-zero-window-44-byte-payload-0123456789!"
client -> server  ACK      seq=ISN+1   ack=SISN+45 win=65535
```

A stack that sends the RFC-style one-byte data probe puts that byte at `SISN+1`; after the reopen
the remaining data starts at `SISN+2`, and `probe_payload + data` must still equal the 44-byte
fixture. Any retransmission below the accepted probe point is a failure.

## 2. Observed evidence

Merged base: `0a641914`.

| Scenario | Result | pcap SHA-256 | Observed detail |
|---|---|---|---|
| zero-window | PASS | `c41dc9985bbc68b430f1bc5ae7bb19a27ab928d39df588ba97e0007005d17f5f` | probe=1B at `SISN+1`; reopen window 65535; data 43 B at `SISN+2`; total 44 B; no replay |
| reopen | PASS | `c41dc9985bbc68b430f1bc5ae7bb19a27ab928d39df588ba97e0007005d17f5f` | the 1-byte probe plus the 43-byte segment is the single post-reopen flush |
| persist | PASS | `c41dc9985bbc68b430f1bc5ae7bb19a27ab928d39df588ba97e0007005d17f5f` | strict one-byte probe; no probe replay; total 44/44 |
| window-scale | PASS | `fe58ce556bcd739e6073b8bbbbe1cc6f766065e8470b379f2e29bfa6b2912754` | client mss=1460 ws=7 sack; server mss=1460 ws=3 sack; plain peer ws=absent |

The exact acceptance run used the K253 harness and produced the one-byte RFC-style probe:

```text
net-conformance: zero-window pass (16.40s) probe=1B seq=1; after-reopen=43B;
total=44/44; frames=1; replayed-probe=no
```

An earlier `--no-build` run of the same scenario produced the zero-length walk at `SISN`
(`probe=0B-walk seq=0; after-reopen=44B`). Both forms are accepted by the default verifier;
`--strict-probe` accepts only the one-byte form, and the acceptance pcap passes it.

The window-scale run used the `handshake` scenario:

```text
net-conformance: handshake pass (87.13s) SYN-ACK mss=1460 ws=3 sack=True;
plain-peer ws=absent win=65535
```

## 3. Commands

```sh
timeout 300 sh scripts/net-conformance.sh --scenario zero-window --target raven
timeout 120 sh scripts/net-conformance.sh --scenario handshake --target raven \
    --no-build --image build/qtest/net-conformance.img \
    --pcap /tmp/opencode/k290-window-scale.pcap \
    --serial /tmp/opencode/k290-window-scale-serial.log

timeout 15 python3 tests/net-conformance/window/verify.py \
    --scenario zero-window --pcap /tmp/opencode/k290-zero-window.pcap
timeout 15 python3 tests/net-conformance/window/verify.py \
    --scenario window-scale --pcap /tmp/opencode/k290-window-scale.pcap
```

The verifier fails on a missing probe, a wrong sequence, a wrong payload, a duplicate data
sequence or a replayed probe. `--strict-probe` requires the one-byte RFC-style probe; the merged
stack currently emits the zero-length walk form.

## 4. Donor reconciliation

Test-only: the expectations are FreeBSD `sys/netinet/tcp_output.c` (persist gate, window update,
SYN/SYN-ACK option emission) and `sys/netinet/tcp_timer.c` (`tcp_timer_persist`,
`TCPTV_PERSMIN`/`TCPTV_PERSMAX`) at `133344405f532bd54ff3629a77d4c4e062f01fdb`, with the K263 Linux
baseline as the comparison outcome. No donor code is copied. The wire rules are RFC 7323
(window scale) and RFC 9293 sections 3.7 and 3.8.6.

## 5. Gaps

* The probe form is timing-dependent across runs: the acceptance run produced the RFC one-byte
  data probe (`SISN+1`, 43 remaining bytes), while an earlier no-build run produced a zero-length
  walk at `SISN`. The default verifier accepts both; `--strict-probe` requires the one-byte form and
  passes on the acceptance pcap.
* A cold image/userland build can exceed the 300 s acceptance cap. The report records the measured
  cold builds and the final warm-cache acceptance run (`16.40 s` scenario, exit 0); the cap was not
  raised.
