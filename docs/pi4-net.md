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
   must expose the `nic:` frame service that `netd` attaches. On the current base these
   preconditions are missing: REL3 is BLOCKED (no `profiles/pi4-release.toml`, and REL1/REL1b,
   J8 and REL2a are not merged), and the GENET driver entry point is still a skeleton. Do not
   claim a network result until they are in the image.
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
sh -n scripts/pi4-net-evidence.sh
scripts/pi4-net-evidence.sh --self-test
```

The `raven-genet` tests cover the gate's order, lease validation, gateway matching, reply
counts and the exact marker transcript. The `genet` tests drive the same gate through
`FrameBackend::observe_link`, proving that the debounced link transition reaches `UMAC_CMD` before
the DHCP and ping observations are accepted.

## UNVERIFIED values

These must come from the owner's serial log, not from the host model or this document:

* the live GENET MMIO/DMA/IRQ resources and the `devd` match;
* the resolved link speed and duplex;
* the DHCP server, lease address, prefix, gateway and DNS;
* the gateway ping sequence, reply count and RTT;
* whether the release image actually includes the GENET `nic:` service and `netd` attaches it;
* any board-only driver fix made in response to those observations.

## Relationship to the other evidence kits

`docs/pi4-genet-board.md` covers the register/PHY/MAC bring-up markers. This document starts at
the debounced link event and ends at the gateway ping. Keep both transcripts in a release report:
the GENET record proves the controller and PHY path, and the `GENET-NET-*` record proves the
network path consumed it.
