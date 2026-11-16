<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 release network evidence fixture

This directory holds the **host-model** transcript and a negative fixture for the REL4 evidence
script. Neither file is Pi 4 board evidence:

* `model-transcript.log` is the stable final transcript shape: devd launch, one GENET start,
  `nic:` service, link, lease, gateway ping and the `GENET-NET-*` sequence. The lines are
  synthetic; they are not board evidence.
* `model-missing-ping.log` removes the ping and completion markers.
* `model-static-qemu.log` uses the QEMU `10.0.2.15/24` / `10.0.2.2` address.
* `model-missing-lease.log` has no DHCP lease or DHCP marker.
* `model-wrong-gateway.log` pings a different address than the lease gateway.
* `model-link-down.log` records a link-down marker instead of link-up.
* `model-duplicate-launch.log` shows two GENET launches.

The script must accept only the positive file; every negative file must fail. `--chain` also
runs `check-chain.sh` before validating the log.

## Model soak

The `pi4-net-model` guest also runs 128 deterministic cycles (seeds `1..=128`) over the host-tested
`NicService`/`FrameBackend`: link flap, DHCP timeout/retry, lease renewal, B39 ring wrap, one TX
drop and a malformed RX descriptor. A successful cycle must produce the four `GENET-NET-*`
markers and leave the RX ring armed with no TX slot outstanding; a failed cycle must have no
transcript. The qtest prints the cycle/fault counts and `markers=ok leaks=0`.

Run the pair without QEMU or a board:

```
scripts/pi4-net-evidence.sh --self-test
```

The board owner replaces the synthetic markers with the serial lines from the owner-run Pi 4
release image and keeps the `netd: interface eth0 ...` plus `ping: ...` lines. A file with
placeholders or without every marker fails the script; see `docs/pi4-net.md`.
