<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K290 TCP window conformance

These scenario descriptions pin the exact TCP window behaviour expected from the merged stack.
They are consumed by `scripts/net-conformance.sh` with the K253 harness; `verify.py` independently
reads the generated pcap and asserts the sequence numbers, flags and payload bytes.

## Scenarios

| File | Harness scenario | What is proven |
|---|---|---|
| `window-scale.toml` | `handshake` | client offers MSS 1460/WS 7/SACK; server answers MSS 1460/WS 3/SACK; a peer without WS gets no WS |
| `zero-window-probe.toml` | `zero-window` | a zero-window probe precedes the reopen, and the data resumes at the probe point |
| `reopen.toml` | `zero-window` | the queued 44-byte write flushes once after the window reopens |
| `persist.toml` | `zero-window` | the probe is not replayed; the probe byte plus the remaining bytes equal the payload |

## Exact exchange

The absolute ISNs are random; the assertions use the offsets below. `SISN` is the server ISN.

```text
client -> server  SYN      seq=ISN     win=65535  opt mss=1460 ws=7 sack
server -> client  SYN-ACK  seq=SISN    ack=ISN+1  win=65535  opt mss=1460 ws<=14 sack
client -> server  ACK      seq=ISN+1   ack=SISN+1 win=0
server -> client  probe    seq=SISN    len=0      (0B walk; RFC 1B probe at SISN+1 is also accepted)
client -> server  ACK      seq=ISN+1   ack=SISN+1 win=65535
server -> client  data     seq=SISN+1  len=44     payload="K253-zero-window-44-byte-payload-0123456789!"
client -> server  ACK      seq=ISN+1   ack=SISN+45 win=65535
```

A 1B probe is the first payload byte: after the reopen the remaining data starts at `SISN+2`, and
`probe_payload + data` must equal the 44-byte fixture. Any retransmission of a byte below the
accepted probe point is a failure.

The exact acceptance run produced the RFC one-byte data probe at `SISN+1` (43 remaining bytes at
`SISN+2`, hash `c41dc9985bbc68b430f1bc5ae7bb19a27ab928d39df588ba97e0007005d17f5f`). An earlier
no-build run produced a zero-length walk at `SISN` (44 bytes at `SISN+1`,
hash `47db0c2070e1631006b4a01f75be42cca91ab43e68200eb471591ce53c249f6c`). The default verifier
accepts both forms; `--strict-probe` requires the one-byte form and passes on the acceptance pcap.

## Commands

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

`verify.py --scenario persist` accepts either probe form; add `--strict-probe` to require the
one-byte data probe. The verifier exits non-zero on a missing probe, a wrong sequence, a wrong
payload, a duplicate data sequence, or a replayed probe.
