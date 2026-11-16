<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 GENET release network (REL4)

Status: **host model and evidence kit only. No Pi 4 board result is claimed.** The GENET link,
DHCP lease and gateway ping have not run on hardware; every value below is UNVERIFIED until the
owner supplies the serial transcript and this script passes on it.

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

1. **Platform launch.** `userland/servers/devd/src/main.rs`, `run_dtb()` only prints the DT
   matches; it does not grant MMIO/DMA/IRQ or spawn a platform driver. The GENET driver cannot
   start until that path launches `/sbin/genet` with the same `bar`/`dma`/`irq` tokens the PCI
   path uses.
2. **Profile.** `profiles/pi4-release.toml` (REL3) includes `netd`, `resolvd` and `ping` but no
   `genet` program or service manifest. Add the program and a service entry that waits for
   `logd`, `platd` (`plat:mac`) and the driver's `nic:` readiness. This is outside REL4a Owns.
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

## Evidence script

```
scripts/pi4-net-evidence.sh <serial.log>
scripts/pi4-net-evidence.sh --capture <tty> <serial.log> [seconds]
scripts/pi4-net-evidence.sh --self-test
```

`--capture` uses `stty -F` and `timeout cat`, like `scripts/pi4-genet-evidence.sh`. The script
prints the repository commit and the log SHA-256, checks the three ordered markers, the
completion marker, the `netd` lease line and a ping summary with at least one reply, then prints
`PASS` or `FAIL`. It never edits the repository.

`--self-test` checks the host-model fixture in `tests/fixtures/pi4-net/` and requires the
negative fixture to fail. That proves the script's parser; it does **not** prove the board path.

## Board procedure (owner run)

1. **Preconditions.** A `pi4-release` profile must exist and include the GENET driver, `netd`,
   the DHCP client path and `ping`; `devd` must grant the GENET MMIO/DMA/IRQ handles; the driver
   must expose the `nic:` frame service that `netd` attaches. The driver and `NicService` are in
   place and compile for the target, but `devd` platform launch, the profile entry, the netd
   DHCP-first configuration and the board probe are still missing (see the exact follow-up
   above). Do not claim a network result until they are in the image.
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
5. **Ping.** Run `ping <gateway>` (three probes are enough) and keep the per-sequence and summary
   lines. At least one reply is required; a timeout-only run is a failure, not a partial pass.
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
