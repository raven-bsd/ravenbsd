<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP ECN/PMTU/PLPMTUD conformance

Status: **OBSERVED** on the K275 net-conformance image; **NARROWED** for PMTU and PLPMTUD.
Ticket K311.

## Matrix

| Scenario | Reference | Raven result | Evidence |
|---|---|---|---|
| `ecn` | RFC 3168; FreeBSD `tcp_input.c` ECN negotiation, `tcp_output.c` ECE/CWR; K263 `baselines/linux/run.py:scenario_ecn` | **FAIL — finding:** SYN-ACK echoes ECE (ECN negotiated), but an IP-CE-marked client data segment is ACKed without ECE | `tests/net-conformance/results/latest.pcap` sha256 `08073b643822fc508c05ec5a5b65db6efe6123f989103c7e45e970846237bfcc`; serial `tests/net-conformance/results/latest-serial.log` sha256 `a62b3287b21ba0dac34d18d75f10253c2eaab1b5056c130c54f785862f409343` |
| `pmtu-blackhole` | RFC 1191/4821; FreeBSD `tcp_subr.c:tcp_mtudisc` | narrowed: 44-byte fixture does not observe a path-MTU decrease | harness `NARROWED` entry |
| `plpmtud` | RFC 8899; FreeBSD `tcp_output.c` probe path | narrowed: 44-byte fixture cannot trigger a probe | harness `NARROWED` entry |

## Commands

```text
timeout 180 sh scripts/net-conformance.sh --scenario ecn --baseline linux
  exit 1: SYN-ACK did not echo ECE (baseline setup gap; tcp_ecn=1 in the server netns)

timeout 300 sh scripts/net-conformance.sh --scenario ecn --target raven --no-build \
  --image /tmp/opencode/k275-worktrees/origin-master/build/qtest/net-conformance.img
  exit 1: CE-marked data did not produce an ECE ACK
  pcap=tests/net-conformance/results/latest.pcap
  serial=tests/net-conformance/results/latest-serial.log
```

The target pcap shows:

```text
tos=02 fl=c2 ... SYN      (ECE|CWR, ECT(0))
tos=00 fl=52 ... SYN-ACK  (ECE: ECN negotiated)
tos=03 fl=18 ... data     (IP CE)
tos=00 fl=10 ... ACK      (no ECE: finding)
```

## Finding for the TCP owners

Raven negotiates ECN but does not echo CE on the ACK (`tcp_input` receive-side ECN marking /
`ECN_CE` handling). The scenario must stay failing until that branch is ported; it is not a
harness pass. K270/K308 own the product side.

## Reproduce

The K275 image used here was built at commit `a426dee0` and is reused with `--no-build`; no image
build was needed in this rework. The harness and matrix changes are test-only.
