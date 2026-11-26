<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K311 ECN/PMTU/PLPMTUD wire scenarios

Status: **OBSERVED** for the ECN scenario; **NARROWED** for PMTU/PLPMTUD.

The scenarios are wired into `tests/net-conformance/harness.py` and the K275 matrix
(`tests/net-conformance/harness/run.py`). The fixture remains the K253 listener; it writes 44
bytes, which is enough for ECN negotiation/CE marking but below the path-MTU and PLPMTUD probe
floor, so those two scenarios are explicitly narrowed rather than faked.

## ECN

| Step | Wire assertion |
|---|---|
| SYN | flags `ECE|CWR`, IP ECT(0) |
| SYN-ACK | `ECE` set (negotiated) |
| client data | IP CE (`0x03`) |
| server ACK | `ECE` set (echo CE) |

Run:

```sh
timeout 300 sh scripts/net-conformance.sh --scenario ecn --target raven --no-build \
  --image /path/to/net-conformance.img
```

The target result is recorded in `docs/tcp-ecn-conformance.md`; a missing ECE echo is a finding,
not a pass.

## PMTU / PLPMTUD

`pmtu-blackhole` needs a router namespace with a reduced MTU and suppressed ICMP frag-needed.
`plpmtud` needs a write above the probe floor. The 44-byte fixture cannot observe either, so both
are in the harness `NARROWED` map with the exact reason.
