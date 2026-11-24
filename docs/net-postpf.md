<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Post-K273 release network regression (K371)

Status: **OBSERVED** on x86_64. The default release profile boots on the K194 tap with
`netd: pf absent` and reaches `netd: ready`; host `ping 10.0.2.15` and guest
`ping 10.0.2.2`/`ping 1.1.1.1` pass. The `pf` profile loads `rules=3 nat=1`, allows the
ICMP gateway flow, allows the stateful UDP echo flow and drops the quick-logged UDP 47002
flow. The raw logs are in `tests/evidence/network/postpf/`.

## Commands

```sh
cargo xtask image --arch x86_64 --profile release --output build/qtest/net-postpf.img
cargo xtask qtest --case net-postpf-default --arch x86_64
cargo xtask qtest --case net-postpf-pf --arch x86_64
make check-fast
```

The release image build takes the measured 600 s cap in this clone (the ticket's 300 s
cap expired in the release user build; the 300 s log is preserved in the report). The two
qtests use the ticket's 300 s cap. The tap proof needs passwordless sudo plus `ip`,
`iptables`, `dnsmasq` and `sysctl`; `sh scripts/qemu-net.sh --check` passes before the run.

## Default release profile

`net-postpf-default` proves two things:

1. The pre-step (`tests/qtest/cases/net-postpf/default.sh` + `tap.py`) boots
   `build/qtest/net-postpf.img` on the K194 tap, brings up the bridge/tap, logs in as root,
   runs host `ping -c 3 10.0.2.15`, guest `ping -c 3 10.0.2.2` and guest `ping -c 3 1.1.1.1`,
   and checks the netd lines before tearing the tap down.
2. The qtest boots the same release profile on QEMU user networking and proves the getty
   login path is unchanged.

The tap serial (`tests/evidence/network/postpf/default/serial.log`) contains:

```text
netd: pf absent
netd: interface eth0 10.0.2.15/24
netd: ready
--- host ping 10.0.2.15 ---
3 packets transmitted, 3 received, 0% packet loss, time 2003ms
ping: 3 packets transmitted, 3 received, 0% packet loss
ping: 3 packets transmitted, 3 received, 0% packet loss
```

The first guest line is 10.0.2.2; the second is 1.1.1.1. The default profile has no
`/etc/pf.conf`; `netd: pf absent` is the intended no-filter path, not a fail-closed start.
`netd: pf loaded` is forbidden in `net-postpf-default`, so a release image that accidentally
stages a pf ruleset fails the case.

### Harness finding

The pre-existing `tests/scripts/qemu-net-selftest.sh` `guest()` waits for `raven# ` after
sending the ping. The release image's brush shell redraws that prompt after every character,
so the wait returns before the ping output and the case fails with only the first byte echoed.
The first failure is preserved in
`tests/evidence/network/postpf/default/selftest-failure.log` and
`selftest-failure-qtest.log`. K371 owns `tests/qtest/cases/net-postpf*`, so the new
`tests/qtest/cases/net-postpf/tap.py` instead waits for the **ping output**
(`0% packet loss`) and paces serial writes one byte at a time. The existing selftest is not
edited.

## pf profile

`net-postpf-pf` boots `profiles/pf.toml` and `profiles/pf.d/pf.conf` with the host UDP echo
(47001) and NAT echo (47004), then runs the one-shot pf probe. The serial
(`tests/evidence/network/postpf/pf/serial.log`) contains:

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

`netd: ping 10.0.2.2` is the allowed ICMP flow through `set default pass`; the UDP 47001
flow is the allowed stateful flow; the quick-logged 47002 flow is the denied flow and the
`netd: pf block` line is the counter/log record.

## Donor reconciliation

Test/evidence only; no product or donor code is copied. The hook under test is the K273
port: FreeBSD `sys/netpfil/pf/pf.c` `pf_test`/`pf_test_rule` (BSD-2-Clause) is the recorded
K273 donor map; the merged test target is `userland/servers/netd/src/filter.rs` and
`crates/raven-pf`. Illumos/IPF and Linux netfilter are outcome references only for the tap
helper (`ip`/`iptables`/`dnsmasq`), which is first-party shell. No `PROVENANCE.toml` entry is
needed.

## Limits

* The tap proof requires root/passwordless sudo; `--check` refuses cleanly without the tap
  prerequisites.
* The evidence is the QEMU serial transcript plus the host/guest ping lines. The K194 helper
  owns the tap/QEMU lifecycle, so no separate host pcap is captured; the pf block record is
  the packet-level log line.
* The default release's `resolvd` lease is not part of this ticket; K227/K203 cover DNS.
* The pf profile's NAT flow (47004) is exercised by the same probe; it is not a K371
  requirement.
