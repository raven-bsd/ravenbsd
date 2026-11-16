<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 soak fixture (O6)

This fixture is the `/init` of the `pi4-soak` qtest. It links the host-testable GENET and PWM
audio models and runs them together for 1024 deterministic iterations. It does not map MMIO, DMA
or any Pi 4 register.

## Build

```
sh tests/fixtures/pi4-soak/build.sh
```

The script builds `raven-pi4-soak` for `x86_64-unknown-none` and packs
`build/fixtures/pi4-soak/initfs.img` with `raven-initfs`.

## Model loop

Each iteration:

1. Posts one RX descriptor and queues one TX descriptor.
2. Feeds two alternating link samples to the debounced `LinkTracker`.
3. Every seventh iteration checks that a still-owned RX descriptor is not advanced.
4. Every eighth iteration completes an RX descriptor with a CRC error; every ninth iteration
   reclaims a TX descriptor with a malformed status.
5. Every 32nd iteration decodes a combined RX/TX interrupt status.
6. Writes one 1024-frame PCM period and services the PWM ring, except every sixteenth iteration,
   which deliberately underruns and must produce silence.
7. After the loop, drives the bounded recovery policy to three ring resets and one fault.

## Expected output

```
pi4-soak: start iterations=1024
pi4-soak: genet rx=1024 tx=910 owned=147 rx_errors=128 tx_errors=114 link_events=1024 irq=32
pi4-soak: recovery resets=3 faults=1
pi4-soak: audio periods=960 frames=983040 underruns=64 writes=1032 bytes=8421504
pi4-soak: pass
```

The exact counters are asserted by `src/lib_ut.rs` and by the qtest case. A failure prints
`pi4-soak: failed <reason>` and exits non-zero.

The fixture is a model soak, not board evidence. See `docs/pi4-soak.md` and
`scripts/pi4-soak.sh` for the UNVERIFIED Pi 4 procedure.
