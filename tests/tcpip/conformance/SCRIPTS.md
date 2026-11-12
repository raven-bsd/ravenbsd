<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Conformance scripts

Status: normative for `raven-tcpip-conformance`. The runner parses the subset
below and drives `raven-tcpip` through the C04 simulator
(`tests/tcpip`). Results live in `CONFORMANCE.md`.

## Provenance

The scripts and the runner are fresh BSD-3-Clause work written for this
project. packetdrill itself is GPL-2.0, tier C in
`architecture/12-licensing-and-provenance.md`: it is a tool whose **script
language** we accept, never code we copy, link or vendor. No packetdrill source
or third-party script is present in this directory. `tests/net/**` (B18's
separate harness) is not modified.

## Topology and time

| Side | IPv4 | MAC | TCP port | ISN |
|---|---|---|---|---|
| SUT (`raven-tcpip`) | `10.0.0.1/24` | `02:00:00:00:00:0a` | `8080` | learned from its SYN / SYN-ACK |
| Scripted peer | `10.0.0.2` | `02:00:00:00:00:0b` | `12345` (default) | `1000` |

Everything runs on virtual milliseconds. Absolute timestamps (`0.100`) and
relative ones (`+0.100`) are accepted; time never goes backwards. Inbound
packets are injected directly into the SUT receive path, outbound packets are
read from the simulator trace.

## Scenario lines

```
# comment
0.000 socket() = 3
0.000 bind(3, ..., 8080) = 0
0.000 listen(3, 1) = 0
+0.100 < S 0:0(0) win 65535 <mss 1460,sackOK,nop,wscale 7>
+0.050 > S. 0:0(0) ack 1
+0.050 < . 1:1(0) ack 1 win 65535
+0.000 accept(3, ..., 0) = 4
```

* `<` is a packet from the scripted peer into the SUT. `>` is a packet expected
  from the SUT. The flag letters are `S F R P . E C U` (`.` is ACK).
* `seq:end(len)` is relative to the peer ISN for `<` and to the learned SUT ISN
  for `>`. `*` matches any SUT sequence number (used for SYN-cookie ACKs whose
  ISS is a keyed value).
* `ack N` is relative to the learned SUT ISN for `<` and to the peer ISN for
  `>`. `win N` is exact when present; omit it when the window is not under
  test.
* `<...>` lists TCP options that must be present. Supported spellings:
  `mss N`, `wscale N`, `sackOK`, `ns`, `ts val N ecr N`, `tsAny`, `nop`,
  `eol`, and a bare numeric kind for an unknown option.
* Syscalls: `socket`, `bind`, `listen`, `accept`, `connect`, `write`, `read`,
  `close`, `setsockopt`, `keepalive`, `peerport`, `sysctl`. `...` is accepted
  for arguments the runner does not need. `= N` on `read` is checked; `-1`
  means any error (including `EAGAIN` and `ECONNRESET`).
* `peerport(N)` changes the scripted peer source port for later `<` packets.
  This is how two distinct half-open entries are created in one script.
* `sysctl("tcp.name", N)` calls `Stack::set_property`; this is how production
  tunables are driven without a separate configuration file.
* `keepalive(N)` calls `Stack::tcp_set_keepalive` with an interval in ms.

## Verdicts

A script whose final expected packet is missing, whose fields differ, or which
leaves an unexpected SUT TCP packet after the last expectation fails. The
runner exits non-zero if any script marked `pass` in `CONFORMANCE.md` fails, or
if the table and the script directory disagree. Scripts marked `fail` or
`skip` are committed as such; a known failure that starts passing is reported
for a deliberate table update, never hidden.

`CONFORMANCE.md` is regenerated deliberately with
`cargo run -p raven-tcpip-conformance -- --write-table`; the committed table is
reviewed, not generated during CI.
