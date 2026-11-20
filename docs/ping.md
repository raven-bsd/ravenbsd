<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# ping

`ping` sends real ICMP echo requests through `netd` and the `net:icmp/<ipv4>` socket. It does not
synthesize replies or RTTs.

```
ping [-c count] [-i seconds] [-w seconds] [-s bytes] host
```

* `-c` — echo count, default 4, range 1-1000.
* `-i` — interval between echo requests, default **1 second**, range 1-60. The first probe is
  immediate; each later probe waits until the interval has elapsed since the previous send. A
  four-probe run therefore takes at least three seconds instead of sending a burst.
* `-w` — overall budget in seconds, default 5, range 1-60.
* `-s` — payload bytes, default 32, range 0-1400.

The guest prints the measured wait before each probe after the first:

```
ping: 10.0.2.2 seq=1 rtt=0ms
ping: interval seq=2 waited=1000ms
ping: 10.0.2.2 seq=2 rtt=0ms
...
ping: 4 packets transmitted, 4 received, 0% packet loss
ping: rtt min/avg/max = 0/0/0 ms
```

A 100% loss run still prints the standard transmitted/received/loss summary and exits **1**.
A successful run exits **0** only when every requested probe was sent and answered.

## Reverse ping (host to guest)

QEMU's default `-netdev user` is a NAT/slirp network. It forwards guest-initiated outbound
traffic and selected inbound TCP/UDP host forwards; it does **not** forward inbound ICMP. A host
`ping 10.0.2.15` therefore cannot reach the guest through slirp, and `hostfwd` does not change
that. This is a documented limitation, not a product failure and not a pass.

To test host-to-guest ICMP, use a real L2 path:

* **Host tap/bridge (QEMU):** create a host bridge (`br0`) and tap device, then start QEMU with
  `-netdev tap,id=n0,ifname=tap0,script=no,downscript=no -device virtio-net-pci,netdev=n0`.
  Give the guest an address on that L2 segment (DHCP from the host bridge or a static address),
  and ping the guest address from the host. The host firewall and bridge must allow ICMP between
  the bridge and the tap.
* **Real Pi 4 LAN:** boot the Pi 4 on the same L2 network; ping its address from the host. This is
  the board/LAN setup, not a QEMU slirp claim.

The opt-in host-to-guest check is not part of the default suite because the runner has no tap or
bridge. Run it only on a host that provides one and record the exact interface/address and packet
counts.

## Tests

```
timeout 300 cargo xtask qtest --case ping-interval --arch x86_64
timeout 300 cargo xtask qtest --case release-login --arch x86_64 --no-build
timeout 240 make check-fast
```

`ping-interval` sends four probes to the slirp gateway, asserts four real `rtt` replies and the
measured `waited=1xxxms` spacing, then runs a one-probe 100% loss command and asserts
`ping-status:1`. The release-login and network cases continue to use the same real ICMP path; the
new `ping: interval ...` lines are additive.

## Donor reconciliation

No donor code was copied. FreeBSD/NetBSD `ping` sleeps between probes by default and reports the
standard summary; that behaviour is the reference. Illumos and Linux use the same outcome. The
Raven implementation remains first-party.

Donor revisions for the report: `freebsd 133344405f532bd54ff3629a77d4c4e062f01fdb`,
`netbsd 36eddebcc36cb0775aa2962005b9c05419a6ff01`,
`illumos-gate 586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`,
`linux adc218676eef25575469234709c2d87185ca223a`.
