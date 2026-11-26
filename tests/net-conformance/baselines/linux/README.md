<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K263 Linux baselines

Independent Linux-kernel baselines for the wire scenarios. The peer is a
user-space raw-TCP entity on a veth pair; the other end is a normal Python
socket served by the Linux kernel. No K253 harness code is required.

```
sudo -n ip link del k263h            # cleanup from an interrupted run
timeout 180 sudo -n python3 tests/net-conformance/baselines/linux/run.py
timeout 60  sudo -n python3 tests/net-conformance/baselines/linux/run.py \
    --scenario duplicate-ack
timeout 180 sudo -n python3 tests/net-conformance/baselines/linux/run.py --list
```

Each run writes `results/linux/<scenario>.pcap` and `.log`. The script creates
and removes the `k263h`/`k263p` veth pair itself; a stale pair is deleted at
start.

| Scenario | Linux result | Wire evidence |
|---|---|---|
| window-scale | PASS | SYN-ACK mss=1460 ws=7 sack=True |
| window-scale-absent | PASS | SYN-ACK has no WS option |
| duplicate-ack | PASS | fast retransmit at the first unacked seq, 1024 bytes |
| out-of-order-sack | PASS | SACK block covers the third segment; hole retransmitted |
| half-close | PASS | server FIN after peer FIN; peer ACK |
| listener-backlog | PASS | 1/5 SYN-ACKs with backlog 1 and syncookies off |
| ecn | PASS | SYN-ACK ECE |
| sack | PASS | SYN-ACK SACK permitted |
| cubic | PASS | host `tcp_congestion_control=cubic` (capability baseline) |
| pmtu-blackhole | SKIP | needs a router namespace; reason recorded |

The pcap/log evidence is committed under `results/linux/`. The scenarios are
defined in `../scenarios/*.toml` with exact NetBSD donor functions and RFC
references. When K253 lands, the runner can consume those names; the packet
assertions are independent of any K253 API.
