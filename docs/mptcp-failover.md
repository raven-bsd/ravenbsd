<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# MPTCP two-tap failover fixture (MPTCP2c)

Status: **fixture and negative control complete; positive failover acceptance BLOCKED on the
userland `IPPROTO_MPTCP` path and a second usable interface.** The two-tap helper, host peer, QMP
unplug and both cases exist. The single-path negative control passes and a mutation that skips the
unplug makes it fail. The positive `mptcp-failover` case is wired with the final assertions and
fails closed: the merged MPTCP2a/2b Rust core is not exposed through `raven-libc-net`/`netd`, and
the two-interface topology cannot claim two legacy IRQs. No fake result is recorded.

## Topology

| Path | Bridge | Tap | Host peer | Guest address |
|---|---|---|---|---|
| primary | `mptcpbr0` 10.77.1.2/24 | `mptcp-tap0` | 10.77.1.2:25021 | 10.77.1.15/24 |
| backup | `mptcpbr1` 10.77.2.2/24 | `mptcp-tap1` | 10.77.2.2:25021 | 10.77.2.15/24 |

`scripts/mptcp-two-tap.sh` owns the two isolated L2 segments (`up`, `down`, `status`,
`unplug-primary`, `replug-primary`). It needs root; the fixtures call it with `sudo -n`. The K194
`scripts/qemu-net.sh` (10.0.2.0/24 demo network) is unchanged. No NAT, forwarding or DNS is needed:
the host peer is on the same bridge as the guest.

The guest image is the existing `network-config-test` profile with a test-only overlay:
`/bin/mptcp-failover`, its service manifest, a test-only `svcd.conf`, a static
`/etc/raven/conf.d/network.toml` (10.77.1.15/24), and `/etc/mptcp-mode`. The positive case attaches
two `virtio-net-pci` functions; the negative case attaches only the primary.

The host peer (`tests/fixtures/mptcp/host.py`) listens on both addresses with MPTCP sockets
(`protocol 262`). A plain TCP client falls back normally on the Linux host, which is exactly the
negative control: a non-MPTCP connection to an MPTCP-capable peer.

## Unplug and timing

The harness claims the first `-qmp` socket it sees (it uses it for `send-key`), and QEMU serves one
client per monitor. The cases therefore expose **two** monitors:

```text
-qmp unix:build/qtest/mptcp-qmp-harness.sock,server=on,wait=off   # harness
-qmp unix:build/qtest/mptcp-qmp.sock,server=on,wait=off           # fixture host peer
```

The host peer opens the second monitor while the guest is alive and unplugs the primary with
`{"execute":"set_link","arguments":{"name":"n0","up":false}}`. It sends a one-byte liveness tick
every 200 ms for 3 s, unplugs after 1 s, and the guest reads ticks for 2 s before requiring 4 s of
silence. A fixture that never unplugs keeps delivering ticks, so the guest reports
`unexpected-data`.

## Measured negative control

`cargo xtask qtest --case mptcp-failover-negative --arch x86_64` (pass, 300 s case budget):

```text
mptcp: fixture mode=negative
mptcp: negative connected
mptcp: negative go
mptcp: negative sent=65536
mptcp: negative unplug requested
mptcp: negative tick_bytes=6
mptcp: negative died_ms=4014
mptcp: negative ok=1
mptcp: negative bound_ms=6000
mptcp: probe IPPROTO_MPTCP errno=93
mptcp: probe blocked
mptcp: negative done
init exited (0)
```

Host artifact `build/fixtures/mptcp/host.log`:
`host: stream ok bytes=65536 t=0.832`, `host: unplug netdev=n0 ok=1 unplug_ms=0 t=1.834`,
`host: ticks sent=15`. `died_ms` is the bounded silence window; the host peer verified all 65,536
pattern bytes and issued the unplug on the second QMP monitor.

Mutation: replacing the `set_link` call with a no-op gives
`tick_bytes=9`, `negative unexpected-data`, `negative ok=0`; the case fails on the forbidden line.
The unplug is load-bearing.

## Blocked positive acceptance

`cargo xtask qtest --case mptcp-failover --arch x86_64` fails closed on the current base
(`origin/master` `e313c1df`, which merges the MPTCP2a/2b Rust core). The serial names both missing
pieces:

```text
devd: driver virtio-net msix unavailable: bad handle or file descriptor; using legacy irq
devd: driver virtio-net irq line=11
devd: driver virtio-net launch failed: irq: resource busy
netd: nic: open nic:info: no such file or directory; waiting for the driver
mptcp: socket(IPPROTO_MPTCP) errno=93
mptcp: blocked mptcp2b-socket-api
init exited (1)
```

* **Two interfaces**: `netd` attaches one `nic:` and `devd` gives both virtio functions the same
  legacy line when MSI-X is unavailable, so the second function cannot launch.
* **Socket API**: `raven-libc-net` `validate_proto` accepts only protocol 0/6 for `SOCK_STREAM`;
  `IPPROTO_MPTCP` (262) returns `EPROTONOSUPPORT` before any wire traffic. The merged
  `raven-tcpip::MptcpSocket` is a host-testable Rust API; `raven-libc-net`/`netd` do not expose it
  as a guest socket or a path-manager request.

The positive case is not weakened: it still expects the two subflows, backup idle before failure,
byte-exact streaming, `switch_bound_ms=1500`, and `init exited (0)`, and it forbids
`mptcp: blocked`. When the libc/netd `IPPROTO_MPTCP` path and a second usable interface land, the
guest program implements the same protocol against the host peer and the case can go green without
changing the fixture.

## Donor reconciliation

This ticket adds test fixtures only; no product expression is copied. The protocol behaviour the
case asserts is from RFC 8684 and the merged MPTCP core.

| Topic | Illumos (CDDL, read-only) | BSD (permissive) | Linux (GPL, read-only) | Decision |
|---|---|---|---|---|
| Subflow teardown / path failure | `usr/src/uts/common/inet/tcp/tcp.c` `tcp_disconnect` (1895), `tcp_closei_local`/`tcp_close_detached` (847, 858, 942); `tcp_input.c` `tcp_input_listener` (167) | FreeBSD `sys/netinet/tcp_subr.c` `tcp_close`/`tcp_drop` and `sys/netinet/tcp_input.c` `tcp_input` are **absent in this sparse checkout**; the merged NetBSD/FreeBSD TCP port is the behavioural source | `net/mptcp/subflow.c` `__mptcp_close_ssk` and `net/mptcp/protocol.c` are absent from the sparse checkout | First-party fixture; the merged `raven-mptcp::path`/`meta` and the MPTCP2a bridge define the invariant |
| Backup/path manager | no MPTCP; TCP close behaviour only | no MPTCP implementation | `net/mptcp/pm.c`, `pm_netlink.c` (absent); RFC 8684 3.3.8/3.5 | The fixture asserts one active path and backup only after failure; no code copied |
| Two-tap unplug harness | n/a (test-only) | n/a | `tools/testing/selftests/net/mptcp/mptcp_join.sh` (absent in the checkout) | First-party QEMU helper and Python peer; QMP `set_link` is the unplug |

Donor revisions: FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, illumos-gate
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`. The missing Linux/FreeBSD files are recorded rather
than reconstructed from memory.

## Files

* `scripts/mptcp-two-tap.sh` - primary/backup bridges and taps, unplug/replug.
* `tests/fixtures/mptcp/{build.sh,main.c,host.py,start.sh,reaper.sh,service.toml,svcd.conf,network.toml}`.
* `tests/qtest/cases/mptcp-failover.toml` - positive acceptance (BLOCKED, fails closed).
* `tests/qtest/cases/mptcp-failover-negative.toml` - single-path negative control (PASS + mutation).
