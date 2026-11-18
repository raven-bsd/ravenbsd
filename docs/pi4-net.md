<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 GENET release network (REL4)

Status: **host model and QEMU gateway gate (OBSERVED); Pi 4 board result UNVERIFIED.** The GENET
link, LAN DHCP lease and external 8.8.8.8 ping have not run on hardware; every board value below
is UNVERIFIED until the owner supplies the serial transcript and the evidence script passes on it.
The QEMU `pi4-net` gate uses user networking and the 10.0.2.2 gateway, not a board claim.

## NET1 QEMU gateway gate

`cargo xtask qtest --case pi4-net --arch x86_64` boots the `hw-net` server image with the B70
probe and a real QEMU user-network DHCP lease. It asserts:

```
b70: lease eth0 10.0.2.15/24 router=10.0.2.2 dns=10.0.2.3
b70: route default via 10.0.2.2 dev eth0
b70: ping 10.0.2.2 seq=1 rtt=...
b70: ping 3 packets transmitted, 3 received, 0% packet loss
b70: resolv.conf nameserver=10.0.2.3
b70: dns localhost=127.0.0.1
b70: done
```

The lease, route, gateway reply and DNS answer are real QEMU user-net behaviour; there is no
synthetic external reply. The board procedure replaces the 10.0.2.2 gateway with the LAN gateway
and adds `ping 8.8.8.8` plus a DNS lookup through `resolvd`.

The Pi 4 release profile stages `netd` in `dhcp-first` mode, `resolvd`, `ping`, `route` and
`ifconfig`; `profiles/pi4-release.d/20-net.toml` adds the inspection tool. A platform NIC publishes
`eth0` only after a validated lease; a QEMU image keeps the fixed user-mode address.

## Release gate

The release gate is three ordered observations plus one completion marker. The host-tested state
machine lives in `crates/raven-genet/src/release.rs`; `userland/drivers/genet/src/release.rs`
joins it to the frame backend's debounced link event.

| Transcript line | Meaning |
|---|---|
| `GENET-NET-LINK speed=<10\|100\|1000> duplex=<full\|half>` | the debounced PHY link is up and `UMAC_CMD` has the resolved speed |
| `GENET-NET-DHCP addr=<addr>/<prefix> gateway=<gw> dns=<dns\|none>` | `netd` applied a usable unicast DHCP lease |
| `GENET-NET-PING target=<gw> tx=<n> rx=<n> rtt_ms=<n>` | `ping <gateway>` received at least one reply |
| `GENET-NET-OK` | all three observations are present and in order |

The state machine rejects a DHCP step before the link, a ping before DHCP, a non-unicast or
zero lease, a lease with a zero prefix, a ping to an address other than the lease gateway, a ping
with zero requests, impossible reply counts and a ping with no reply. The transcript is stable:
the four lines are emitted in the table order and are the only input the evidence script needs
from the driver.

The script also requires the normal console evidence around the markers:

```
netd: interface eth0 <addr>/<prefix>
ping: <gateway> packets transmitted, <n> received, <loss>% packet loss
```

## GENET `nic:` frame service

REL4a wires the frame backend to the B39 contract that `netd` already consumes:

* `userland/drivers/genet/src/nic.rs` owns the host-tested `NicService`;
* the B39 region is at DMA offset `0x1000`, length `0x6000` (header, 8 RX descriptors,
  8 TX descriptors, eleven 2 KiB pool buffers);
* pool buffers 0..7 are the eight GENET RX slots; pool buffers 8..10 are the three GENET TX
  slots, so GENET and `netd` share the same DMA bytes (no frame copy);
* `userland/drivers/genet/src/main.rs` maps the `devd` BAR/DMA/IRQ handles through
  `PlatformSeam`, performs the typed bring-up, calls `FrameBackend::program_rings`, and serves:

| Path | Meaning |
|---|---|
| `nic:info` | `mac=...`, `mtu=...`, `link=up\|down`, ring and DMA geometry |
| `nic:mac`, `nic:mtu`, `nic:link` | one-line values |
| `nic:ring` | attachment of the DMA window (the B39 region is at `ring_offset`) |
| `nic:start`, `nic:stop` | arm or pause the RX/TX pump |
| `nic:release` | read the evidence transcript, or feed one DHCP/ping probe line |
| `nic:quit` | test-only clean shutdown |

`nic:release` accepts exactly two lines:

```
dhcp <addr>/<prefix> <gateway> <dns|none>
ping <gateway> tx=<n> rx=<n> rtt_ms=<n>
```

A board probe can therefore read the `netd` lease and `ping` result and feed them to the driver;
when all three stages are complete the driver prints the `GENET-NET-*` transcript. The parser and
feed path are host-tested; the probe binary and its profile entry are the board follow-up below.

## netd addressing modes

`netd` selects the address source from the service arguments or `NETD_NIC_MODE`; the QEMU path is
the default:

| Mode | Selection | Address before lease | `eth0` publication |
|---|---|---|---|
| QEMU user | default `netd nic` | `10.0.2.15/24`, gateway `10.0.2.2` | immediately (B70/`hw-net` contract) |
| Platform DHCP-first | `netd nic dhcp-first` or `NETD_NIC_MODE=dhcp-first` | none | only after a validated DHCP lease |

In DHCP-first mode `netd` starts the DHCP client with no IPv4 configuration, validates the lease
(unicast address/gateway/DNS, prefix `1..=32`), and only then publishes `eth0` in `net:iface` and
the console. A malformed lease leaves `eth0` unpublished. The Pi 4 release profile must pass the
`dhcp-first` token (or set the environment value); the profile is outside REL4b's Owns.

When `netd` has a live lease, it writes the matching `dhcp <addr>/<prefix> <gateway> <dns|none>`
line to `nic:release`; when an ICMP echo reply arrives from the lease gateway, it writes the
`ping <gateway> tx=1 rx=1 rtt_ms=<n>` line. The QEMU `virtio-net` driver has no `nic:release`
endpoint, so those writes are ignored on the existing `hw-net` path.

## Board follow-up (exact)

1. **Platform launch.** P8 now implements `devd dtb launch` and transfers BAR slot 3, DMA slot 4
   and IRQ slot 5. The GENET driver accepts that order only when the manifest selects it
   (`platform` or the explicit `bar_handle=`/`dma_handle=`/`irq_handle=` tokens); the default is
   the PCI/QEMU order (BAR 3, config 4, DMA 5, IRQ 6). Add the platform selector to the GENET
   manifest in the installed `/etc/drivers.toml` (`["bar=0", "resources=dma+irq", "platform",
   "serve"]`). This is outside REL4c Owns.
2. **Profile.** `profiles/pi4-release.toml` (REL3) includes `netd`, `resolvd` and `ping` but no
   `genet` program or service manifest. Add the program, the platform manifest selector above
   and a service entry that waits for `logd`, `platd` (`plat:mac`) and the driver's `nic:`
   readiness. This is outside REL4c Owns.
3. **netd mode.** `userland/servers/netd/src/nic.rs` now keeps the QEMU constants only in
   `NicMode::QemuUser`; `NicMode::DhcpFirst` uses `NicConfig::platform` and waits for the lease.
   The remaining profile step is to pass `dhcp-first` (or set `NETD_NIC_MODE`) in the Pi 4 netd
   service; the QEMU default is unchanged.
4. **Ring values.** `FrameBackend::program_rings()` writes the documented Pi 4 ring setup
   (`RING_BUF_SIZE`, start/end, producer/consumer, DMA control, UMAC RX/TX enable). The exact
   register values and the descriptor producer/consumer index programming remain UNVERIFIED
   until the board log exists.
5. **Live device tree.** `userland/drivers/genet/src/main.rs`, `pi4_info()` uses the documented
   Pi 4 base `0xfd58_0000`, MDIO `0xe14`, PHY 1 and `rgmii-rxid`; the board must confirm the live
   DT values and the `plat:mac` result.

## Launch/readiness chain

`tests/fixtures/pi4-net/check-chain.sh` reads the resolved `pi4-release` profile. Its matrix is:

| Assertion | Expected |
|---|---|
| `devd.manifest` args | `dtb launch` |
| GENET driver args | `slots=platform` |
| GENET launchers | exactly one, owned by `devd` |
| `netd.manifest` needs | `scheme:nic` |
| `/etc/pi4.dtb` staged | yes |
| `/etc/drivers.toml` staged | yes |

Observed on this base (`2026-11-16`, after P10/P12): `devd launch`, driver slots, single GENET
launcher, netd dependency, DTB and driver staging all pass. The earlier duplicate svcd
`genet.manifest` launcher was removed by the profile follow-up; the host check now prints
`genet_launchers=1`. The `pi4-net-model` qtest still proves the checker accepts the coherent
model and rejects a duplicate-launcher model.

## Failure modes

| Symptom | Likely cause | Check |
|---|---|---|
| no `nic:` / netd stuck | devd launch missing, duplicate/late GENET, or driver exited before scheme registration | `svcd` status, `devd: dtb launch genet ...`, `/etc/drivers.toml` |
| no `eth0` / no DHCP marker | link down, no lease, or netd started in the wrong mode | `GENET-NET-LINK`, `netd: interface eth0 ...`, `GENET-NET-DHCP` |
| driver exits before `nic:` | `plat:mac` missing or invalid, MMIO/DMA/IRQ grant rejected | `genet: ...` serial line and `platd` status |
| link marker missing | PHY/cable/autonegotiation failure | `GENET-REV`/`GENET-PHY`/`GENET-RGMII` and `GENET-NET-LINK` |
| ping marker missing | no lease gateway reply, wrong gateway, or ping not run | `GENET-NET-DHCP`, `ping` summary, `GENET-NET-PING` |

## Model soak

`tests/fixtures/pi4-net/model` runs 128 deterministic cycles over the host-tested
`NicService`/`FrameBackend` with seeds `1..=128`. The cycle selector injects:

| Case | Trigger | Expected |
|---|---|---|
| plain success | remaining seeds | four `GENET-NET-*` markers, RX armed, TX empty |
| link flap | `seed % 17 == 0` | recover and complete |
| DHCP timeout | `seed % 19 == 0` | no lease, no `GENET-NET-OK` |
| lease renewal | `seed % 13 == 0` | new lease and completion |
| B39 ring wrap | `seed % 31 == 0` | wrap RX/TX and complete |
| TX drop | `seed % 23 == 0` | rejected descriptor, no `GENET-NET-OK` |
| malformed RX | `seed % 29 == 0` | error counted, then complete |

The `pi4-net-model` qtest prints `pi4-net-soak: cycles=128 ok=... failed=...`,
`faults ...`, `markers=ok leaks=0` and `pass`. A failed cycle never produces a transcript or
`GENET-NET-OK`; a successful cycle leaves the RX ring at 8 posted descriptors and the TX ring
empty. This is a host model, not a board claim.

## Evidence script

```
scripts/pi4-net-evidence.sh <serial.log>
scripts/pi4-net-evidence.sh --capture <tty> <serial.log> [seconds]
scripts/pi4-net-evidence.sh --chain <serial.log>
scripts/pi4-net-evidence.sh --self-test
```

`--capture` uses `stty -F` and `timeout cat`, like `scripts/pi4-genet-evidence.sh`. The script
prints the repository commit and the log SHA-256, then requires the final chain:

```
devd: dtb launch genet ...
genet: revision=... slots=Platform
genet: serving nic:
netd: interface eth0 <lease>/<prefix>
GENET-NET-LINK speed=<...> duplex=<...>
GENET-NET-DHCP addr=<lease>/<prefix> gateway=<gw> dns=<...>
ping: <gw> ...
GENET-NET-PING target=<gw> tx=<n> rx=<n> rtt_ms=<n>
GENET-NET-OK
```

Exactly one devd launch and one `genet: serving nic:` are required. The script rejects the QEMU
user-mode address (`10.0.2.15/24` / gateway `10.0.2.2`), a missing lease, a ping target or
summary host that differs from the lease gateway, a link-down marker, an out-of-order marker
sequence and a duplicate GENET launch. `--chain` runs REL4d's `check-chain.sh` first and exits
nonzero if either the profile chain or the log fails. `--self-test` runs the positive and all
negative synthetic fixtures; that proves the parser, not the board path.

## Board procedure (owner run)

1. **Preconditions.** P10 supplies devd's `platform-root` grant, P12 supplies `platd` and the
   GENET MAC, and REL3 stages the `pi4-release` image. Run
   `sh tests/fixtures/pi4-net/check-chain.sh`; it must print `genet_launchers=1` and `PASS`.
   The driver must publish `nic:` and netd must be in DHCP-first mode. Do not claim a network
   result until this chain and the board log both pass.
2. **Build and flash.** Build the release image for `--arch aarch64 --platform pi4 --profile
   pi4-release`, write the FAT/root layout exactly as REL3 documents, and attach the UART at
   115200 8N1.
3. **Boot.** Capture the complete serial log. Confirm the kernel banner, `devd` matched the
   GENET function, the GENET bring-up record reached `Ready`, and `netd` printed the eth0 lease
   line. A missing `GENET-REV`/`GENET-PHY`/`GENET-RGMII` record means the link did not start;
   stop and attach that log.
4. **DHCP.** Confirm `netd` applied a lease. The gate line must name the address, prefix,
   gateway and DNS (or `none`). A static address or an APIPA `169.254/16` address is not a
   DHCP result.
5. **Ping/DNS.** Run `ping <gateway>` and `ping 8.8.8.8` (three probes each are enough) and keep
   the per-sequence and summary lines. At least one reply from each is required; a timeout-only
   run is a failure, not a partial pass. Resolve a name through `resolvd` and record the answer.
6. **Gate the log.**

   ```
   scripts/pi4-net-evidence.sh /tmp/pi4-net-serial.log
   ```

   Attach the `PASS` output, the commit and the log SHA-256 to the report. Without that output
   the board result stays UNVERIFIED.

## Host tests

```
cargo nextest run -p raven-genet
cargo nextest run -p genet
cargo nextest run -p netd
cargo nextest run -p raven-netcore
cargo check -p genet --target aarch64-unknown-none-softfloat
cargo check -p netd --target x86_64-unknown-none
cargo xtask qtest --case hw-net --arch x86_64
sh -n scripts/pi4-net-evidence.sh
scripts/pi4-net-evidence.sh --self-test
```

The `raven-genet` tests cover the gate's order, lease validation, gateway matching, reply
counts and the exact marker transcript. The `genet` tests drive the same gate through
`FrameBackend::observe_link`, proving that the debounced link transition reaches `UMAC_CMD` before
the DHCP and ping observations are accepted. They also cover the B39 ring mapping, RX publishing,
TX queue/reclaim, malformed descriptors, the `nic:release` parser and the target compile.

The `netd` tests cover the address-mode matrix: DHCP-first publishes no address before a lease,
rejects malformed leases and publishes a valid lease; the QEMU mode keeps the fixed user-mode
address; and only a NIC gateway reply produces a release ping marker. The existing `hw-net` QEMU
case remains the end-to-end proof of the QEMU default path.

There is no `pi4-net` QEMU case: QEMU has no GENET v5 model, and adding a fake PCI device would not
exercise the Pi 4 platform path. The frame service runs in the host tests and the target code is
compile-checked; the board run is the evidence the owner must supply.

## NET1 donor reconciliation (DR-57/52)

Revisions from `ravenbsd-resources/donors.lock`: `netbsd 36eddebcc36cb0775aa2962005b9c05419a6ff01`,
`freebsd 133344405f532bd54ff3629a77d4c4e062f01fdb`, `illumos-gate
586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`, `linux adc218676eef25575469234709c2d87185ca223a`.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| DHCP lease validation | `dhcpagent`/`ipadm` lease and interface state machine (CDDL, behaviour only) | NetBSD `sbin/dhcpcd` / `lib/libc` lease handling and route installation (BSD-2/3); FreeBSD `dhclient` (BSD) | `dhcpcd`/`NetworkManager` outcome only | First-party `raven-tcpip` DHCP client + `netd` validation; no donor code |
| Default route | `route` and `ipadm` install a gateway route with the lease | NetBSD `sbin/route` and kernel `rtrequest` semantics (BSD) | `ip route` outcome only | `NetCore::set_ipv4`/route table; `netd` publishes only a validated lease |
| DNS | `resolv.conf`/`resolvd` service model | NetBSD `resolv.conf`/`res_send` (ISC/BSD) | glibc/systemd-resolved outcome only | First-party `resolvd` reading the netd lease; no donor code |
| Ping | ICMP echo through the socket layer | NetBSD `ping` (BSD-3) and ICMP handling | iputils ping outcome only | First-party `raven-ping` + `netd` ICMP core; no donor code |
| Tests | N/A | N/A | N/A | Host netd/resolvd/ping tests + QEMU `pi4-net` (10.0.2.2); board UNVERIFIED |

No `PROVENANCE.toml` import is added because no donor file was copied or transpiled.

## UNVERIFIED values

These must come from the owner's serial log, not from the host model or this document:

* the live GENET MMIO/DMA/IRQ resources, the `devd` match and the platform launch;
* the resolved link speed and duplex;
* the DHCP server, lease address, prefix, gateway and DNS;
* the Pi 4 profile's `dhcp-first` selection and the lease it receives;
* the gateway ping sequence, reply count and RTT;
* the `FrameBackend::program_rings` values and the hardware producer/consumer indices;
* whether the release image includes the GENET `nic:` service and `netd` attaches it;
* whether the `nic:release` probe receives the real netd lease and ping result;
* any board-only driver fix made in response to those observations.

## Relationship to the other evidence kits

`docs/pi4-genet-board.md` covers the register/PHY/MAC bring-up markers. This document starts at
the debounced link event and ends at the gateway ping. Keep both transcripts in a release report:
the GENET record proves the controller and PHY path, and the `GENET-NET-*` record proves the
network path consumed it.
