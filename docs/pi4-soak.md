<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 integrated network and audio soak (O6)

Status: host-model qtest plus a board evidence helper. **No Pi 4 board run was performed for
O6.** The qtest runs the GENET and PWM audio models inside an x86-64 guest; it proves the shared
loop and counters, not the Pi 4 hardware path. Every board field below is UNVERIFIED until a log
and an audio recording are attached to the ticket report.

## 1. What runs

`tests/fixtures/pi4-soak` is a `no_std` `/init` that links the host-testable models:

* `raven-genet`: RX/TX descriptor ownership, ring wrap, malformed RX/TX completions, debounced
  link flaps, interrupt-status decode and the bounded recovery budget.
* `pwm-audio` + `raven-pwm` + `raven-proto-audio`: the B40 PCM ring, duty conversion, the four-CB
  DMA ring, period service and underrun silence.

The same library code runs under `cargo test -p raven-pi4-soak` and as the qtest guest. One
iteration therefore checks both models together, including the order in which network and audio
work is serviced.

### 1.1 Iteration shape

| Step | Every iteration | Frequency |
|---|---|---|
| Post one RX descriptor, queue one TX descriptor | yes | 1 |
| Two alternating debounced link samples | yes | 2 |
| Still-owned RX descriptor must not advance | yes | every 7th |
| RX completion with CRC error | yes | every 8th |
| TX reclaim with malformed status | yes | every 9th |
| Combined RX/TX interrupt status decode | yes | every 32nd |
| Write 1024-frame PCM period and service the audio ring | yes | except every 16th |
| Inject one audio underrun and play silence | yes | every 16th |

After the loop, the GENET recovery policy is driven through three ring resets and one fault. The
audio header's underrun counter must match the injected count, and both GENET rings must be empty
(`posted() == 0`).

### 1.2 Expected model counters (1024 iterations)

| Counter | Value |
|---|---|
| RX completions | 1024 |
| TX completions (valid) | 910 |
| Still-owned checks | 147 |
| RX errors (CRC) | 128 |
| TX errors (malformed) | 114 |
| Link events | 1024 |
| IRQ decode checks | 32 |
| Recovery resets / faults | 3 / 1 |
| Audio PCM periods | 960 |
| Audio frames | 983 040 |
| Audio underruns | 64 |
| PWM DMA writes | 1032 |
| PWM DMA bytes | 8 421 504 |

The writes are four duty periods plus four control blocks during `prepare`, then one duty period
per service call (including the silence period for an underrun).

## 2. Run the host test and qtest

```sh
cargo test -p raven-pi4-soak
cargo xtask qtest --case pi4-soak --arch x86_64
```

The qtest builds `tests/fixtures/pi4-soak/build.sh` into `build/fixtures/pi4-soak/initfs.img`,
boots the `base` x86-64 image with that initfs and requires the exact marker lines in section 3.
A panic, an `EXCEPTION`, or `pi4-soak: failed` fails the case.

The host test asserts the same report. It does not replace the qtest: the guest run also proves
that the fixture links and executes as a `no_std` userland process.

## 3. Expected qtest serial output

```text
pi4-soak: start iterations=1024
pi4-soak: genet rx=1024 tx=910 owned=147 rx_errors=128 tx_errors=114 link_events=1024 irq=32
pi4-soak: recovery resets=3 faults=1
pi4-soak: audio periods=960 frames=983040 underruns=64 writes=1032 bytes=8421504
pi4-soak: pass
```

## 4. Pi 4 board procedure (UNVERIFIED)

The board path needs a Pi 4 image/profile that runs the same soak entry point and emits the
markers above. `profiles/pi4*` and the GENET/PWM target drivers are outside O6's Owns list, so this
ticket does not add that wiring. The owner runs the procedure when the profile exists.

### 4.1 Firmware

Use the B41/B46 audio settings and the O5 GENET link:

```text
dtparam=audio=off
dtoverlay=pwm1,pins_40_41,pull_off
```

Confirm the live overlay effect and that the firmware audio service is not driving GPIO40/41. Do
not use `audio=on` at the same time as the PWM driver.

### 4.2 Boot and soak

1. Build or obtain the Pi 4 image containing the soak entry point; record its commit and SHA-256.
2. Connect the 3.5 mm jack to a PC line input and the GENET port to a known peer.
3. Boot the board and start the soak (the exact command is profile-defined and is not part of
   O6). The soak must print the section 3 markers.
4. Capture the UART while it runs:

```sh
scripts/pi4-soak.sh --capture /dev/ttyUSB0 build/pi4/pi4-soak.log 600
```

5. Check the captured log:

```sh
scripts/pi4-soak.sh build/pi4/pi4-soak.log
```

The helper requires `pi4-soak: start`, `pi4-soak: genet`, `pi4-soak: recovery`, `pi4-soak: audio`
and `pi4-soak: pass`, and fails on `pi4-soak: failed`, a panic or an `EXCEPTION`. It prints the
repository commit and the log SHA-256, then `pi4-soak-evidence: PASS`.

6. Write the evidence template and fill it from the log and the recording:

```sh
scripts/pi4-soak.sh --template build/pi4/pi4-soak-evidence.md
```

### 4.3 Board counters

The script deliberately does not hardcode the model counts: board counters can differ with link
speed, traffic and the profile's iteration budget. Record every observed value and explain a
difference from section 1.2. A hard failure, a stuck `posted()` ring, a recovery fault without a
reset, or a missing underrun counter is a fail even if the `pass` marker is present.

### 4.4 Audio check

1. Play a 1 kHz sine at about -6 dBFS for 5 s at 48 kHz, then 44.1 kHz.
2. Record the jack on a PC line input at 48 kHz/24-bit. Check the fundamental, level, channel
   separation and the absence of periodic clicks.
3. Repeat left-only and right-only to settle the B46 `RightLeft`/`LeftRight` order.
4. Compare with `dtparam=audio=on` as the firmware reference, then with `audio=off` plus the PWM
   driver.

### 4.5 Evidence to attach

* The UART log and its SHA-256.
* The image commit and SHA-256, the final `config.txt`, and the live device-tree/register dump.
* The filled evidence template.
* The line-in recording or spectrogram, capture device and gain.
* Any board counter that differs from the model, with the reason.

Until those exist, the board result is **UNVERIFIED**.

## 5. Gaps and risks

* The qtest exercises the in-memory models, not Pi 4 MMIO, DMA, cache coherency or the GENET PHY.
* The board profile and soak command are outside O6; only the evidence helper is delivered here.
* Link flaps are model samples; real debounce timing and PHY behaviour are UNVERIFIED.
* Audio underruns are injected deterministically; a real board needs a period clock and IRQ path.
* The recovery budget (three resets, then fault) is a model policy, not a measured board value.
* The qtest is x86-64 only; an aarch64 fixture/profile is future integration work.

## 6. Provenance

No donor source was read for O6. The soak uses the clean-room GENET model from O1-O5, the PWM
model and driver core from B46, and the B40 PCM ring. `PROVENANCE.toml` is unchanged.
