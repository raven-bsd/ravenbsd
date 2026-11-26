<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K263 TCP conformance scenarios

These files are the wire-level scenario matrix for K263. They are deliberately
independent of the K253 harness core: the Linux baselines under
`baselines/linux/` consume the same scenario names and assertions today, and
when K253 lands its runner can adapt its schema without changing the wire
expectations.

Each file names the exact NetBSD donor functions that define the expected
behaviour, the RFC reference for the packet rule, the Linux baseline command,
and the expected packet/ACK observations. The baseline writes the pcap/log to
`results/linux/<name>.{pcap,log}`.

Run all Linux baselines with:

```sh
timeout 180 sudo -n python3 tests/net-conformance/baselines/linux/run.py
```

The PMTU black-hole scenario is defined but narrowed: a deterministic path-MTU
black hole needs a router namespace with a reduced MTU and suppressed ICMP
frag-needed; the single veth peer cannot hide the path MTU from the sender.
