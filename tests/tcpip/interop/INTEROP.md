<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Host TAP interoperability

`raven-tcpip-interop` attaches the native stack to a Linux TAP interface and
runs ordinary host clients and servers against it. The default test run never
needs TAP; the ignored scenarios skip with the exact missing prerequisite.

## Address plan

| Side | IPv4 | IPv6 | MAC | TCP port |
|---|---|---|---|---|
| TAP / host | `10.77.0.1/24` | `fd00:77::1/64` | host-assigned | host services |
| `raven-tcpip` | `10.77.0.2/24` | `fd00:77::2/64` | `02:00:00:00:00:99` | `8080`, `9090` |

## One-time setup (owner, outside every test)

Creating a TAP device needs `CAP_NET_ADMIN`. The tests and the `TapDevice`
never call `sudo`, `setuid`, or any privilege-acquisition API.

```sh
sudo ip tuntap add dev rvn0 mode tap user "$USER"
sudo ip link set rvn0 up
sudo ip addr add 10.77.0.1/24 dev rvn0
sudo ip -6 addr add fd00:77::1/64 dev rvn0
# optional host helpers used by the scenarios
sudo sysctl -w net.ipv4.ip_forward=0
```

Run a scenario with the interface name in `RAVEN_TAP`:

```sh
RAVEN_TAP=rvn0 cargo test -p raven-tcpip-interop -- --ignored --nocapture
```

If `/dev/net/tun`, the module, or the interface is absent, the run prints a
`SKIP` line and exits 0; it never fails the default suite.

## Scenario commands

The ignored test names map to these host commands:

* `tcp_server_small_transfer`, `tcp_server_10mib_transfer`: run the interop
  server on `10.77.0.2:8080`, then `nc 10.77.0.2 8080 < data` / `nc -l`.
* `http_client`: `python3 -m http.server 8000 --bind 10.77.0.1` and fetch from
  the stack.
* `echo_client`: `nc -l 10.77.0.1 9000` and an echo loop.
* `ping4`, `ping6`: `ping 10.77.0.2`, `ping6 fd00:77::2`.
* `dns_udp_and_tcp_fallback`: point the stack resolver at `10.77.0.1:53` with
  a local `dnsmasq` that serves a large TXT answer.
* `window_scale_and_timestamps`: `tcpdump -i rvn0 -vv 'tcp port 8080'` and
  inspect the SYN/SYN-ACK options.
* `sack_under_loss`: `tc qdisc add dev rvn0 root netem loss 2%` before a
  10 MiB transfer; inspect the SACK blocks in `tcpdump`.
* `ecn_negotiation`: `sysctl -w net.ipv4.tcp_ecn=1` and inspect ECE/CWR in the
  trace.
* `pmtu_reaction`: `ip link set rvn0 mtu 1200` mid-transfer and inspect the
  resegmentation.
* `rst_in_both_directions`: close with `SO_LINGER=0` on each side and inspect
  the RST.
* `netstat_style_comparison`: compare `ss -tin` on the host with
  `SocketInfo`/`sockets()` from the stack.

## Capture

Use `tcpdump -i rvn0 -w trace.pcap` for the transcript, then commit the
textual `tcpdump -nn -r` output under `transcripts/`. Do not commit PCAP
binaries.
