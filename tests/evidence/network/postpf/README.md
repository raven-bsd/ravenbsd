<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K371 post-K273 network regression evidence

Product code base: `origin/master` `a31195af` plus the K371 test-only commits
`e8c09856`, `e4e7f652`, `1943704f`, `f6d44b59`. No product source is edited.

The commands and acceptance contract are in `docs/net-postpf.md`. The key results:

| Profile | Case | Result | Raw evidence |
|---|---|---|---|
| default release | `net-postpf-default` | **pass**, 219,040 ms, exit 33 | `default/serial.log`, `default/qtest.log`, `default/qtest.json` |
| pf | `net-postpf-pf` | **pass**, 86,729 ms, exit 33 | `pf/serial.log`, `pf/qtest.json` |

## Default release tap

`default/serial.log` is the K194 tap serial from the `tap.py` harness. The host-side ping
statistics and both guest ping summaries are in the transcript:

```text
netd: pf absent
netd: interface eth0 10.0.2.15/24
netd: ready
--- host ping 10.0.2.15 ---
3 packets transmitted, 3 received, 0% packet loss, time 2003ms
ping: 3 packets transmitted, 3 received, 0% packet loss
ping: 3 packets transmitted, 3 received, 0% packet loss
```

`default/up.txt` and `default/down.txt` are the K194 helper contract and cleanup output.
`default/qtest.log` is the release-profile user-network login/getty log.

`default/selftest-failure.log` and `default/selftest-failure-qtest.log` preserve the first
attempt: the pre-existing `tests/scripts/qemu-net-selftest.sh` waited for `raven# `, which
the brush shell redraws after every character, so it returned before the ping output. The
K371-owned `tap.py` instead waits for the ping output and paces serial writes.

## pf profile

`pf/serial.log` is the `net-postpf-pf` qtest transcript derived from `profiles/pf.toml`
and `profiles/pf.d/pf.conf`:

```text
netd: pf loaded rules=3 nat=1
netd: ready
netd: ping 10.0.2.2 rtt=50ms
pf: allowed flow ok
pf: blocked datagram sent
netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match
pf: ok
svcd: settled
init exited (0)
```

The allowed ICMP flow is `netd: ping 10.0.2.2`; the allowed stateful UDP flow is
`pf: allowed flow ok`; the denied flow and its log record are the `pf: blocked datagram sent`
and `netd: pf block` lines.

## Host prerequisites

The tap run used passwordless `sudo` and passed `sh scripts/qemu-net.sh --check`
(`ip`, `iptables`, `dnsmasq`, `sysctl`; uplink `enp58s0`; bridge/tap free before the run).
The release image is `build/qtest/net-postpf.img` (335,544,320 bytes) built from the clean
`a31195af` product tree; the ticket's 300 s image cap expired in the release user build and
the measured 600 s run passed.

## Gate

`make check-fast` and `make evidence-check` are recorded in
`tickets/tim/reports/K371.md`; the report carries the `## Evidence (ASPICE/ASIL)` block.
