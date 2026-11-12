<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Interop transcripts

Host: Linux 5.19.0-50-generic x86_64. Tools: curl 7.81.0, OpenBSD netcat
1.218, iputils 20211215, iproute2 5.15.0.

TAP setup was **not performed** in this wave. Creating `/dev/net/tun` state
needs `CAP_NET_ADMIN`; tests must not call `sudo` or acquire privilege. The
exact prerequisite is `RAVEN_TAP` unset; `TapDevice::open` also reports
`/dev/net/tun is missing` or the ioctl error if the interface is absent.

| Scenario | Verdict | Reason |
|---|---|---|
| tcp_server_small_transfer | blocked | RAVEN_TAP unset; TAP setup needs owner privilege |
| tcp_server_10mib_transfer | blocked | RAVEN_TAP unset; hash check not run |
| http_client | blocked | RAVEN_TAP unset |
| echo_client | blocked | RAVEN_TAP unset |
| ping4 | blocked | RAVEN_TAP unset |
| ping6 | blocked | RAVEN_TAP unset |
| dns_udp_and_tcp_fallback | blocked | RAVEN_TAP unset |
| window_scale_and_timestamps | blocked | RAVEN_TAP unset; no real trace to inspect |
| sack_under_loss | blocked | RAVEN_TAP unset; no tc loss applied |
| ecn_negotiation | blocked | RAVEN_TAP unset |
| pmtu_reaction | blocked | RAVEN_TAP unset |
| ipv6_transfer_and_ping | blocked | RAVEN_TAP unset |
| rst_in_both_directions | blocked | RAVEN_TAP unset |
| netstat_style_comparison | blocked | RAVEN_TAP unset |

No interoperability defect was observed because no real peer ran. The exit
path is clean:

```
cargo test -p raven-tcpip-interop                 # 1 pass, 14 ignored
cargo test -p raven-tcpip-interop -- --ignored    # 14 pass/skip, exit 0
```

The ignored bodies print `SKIP <scenario>: RAVEN_TAP is not set ...` with
`--nocapture`. This file must be replaced with real transcripts when the owner
runs the TAP setup in `INTEROP.md`.
