<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release login cold-start baseline

`scripts/login-cold-start.sh` boots the x86_64 `release` image under QEMU and measures the two
login steps on the serial console:

1. `login:` -> `Password:` after sending `root`;
2. `Password:` -> `raven# ` after sending `raven`.

Each sample gets a fresh OVMF vars copy. The script records the host load, acceleration, SMP
count and commit with both step durations, then prints the per-sample table and a summary
(`n`, `min`, `median`, `p90`, `max`). The raw serial transcript for every sample is kept under
`build/login-cold-start/<UTC>/`.

## Usage

```
sh scripts/login-cold-start.sh                                  # KVM -smp 2, KVM -smp 4, TCG -smp 4
sh scripts/login-cold-start.sh --accel kvm --smp 2 --samples 5
sh scripts/login-cold-start.sh --accel tcg --smp 4 --samples 5
sh scripts/login-cold-start.sh --accel kvm --smp 2 --samples 20 --baseline build/login-cold-start/baseline.json
sh scripts/login-cold-start.sh --accel kvm --smp 2 --samples 5 --check
```

* `--samples N` defaults to 20.
* KVM configurations run only when `/dev/kvm` is readable and writable; otherwise the refusal is
  recorded and the remaining configurations still run.
* `--image FILE` selects a prebuilt release image; `--no-build` requires it.
* `--baseline FILE` writes the JSON result (metadata, samples, summary) and, if the file already
  exists, prints the previous median -> current median delta. `--check` is the pass/fail switch.
* `--boot-timeout` and `--step-timeout` bound the boot and each login step (defaults 60 s and 15 s).
  The outer command still needs its own proportional `timeout`.

The image is reused when `build/login-cold-start/release.img.commit` matches the current commit;
otherwise `tests/release/check-loader.sh` runs and `cargo xtask image --profile release` rebuilds it.

## Check threshold

`--check` exits nonzero when any configuration has fewer than `--samples` successful samples, any
sample failed, or the **median or p90** of either login step is above **1000 ms**. A failing run
is not retried to hide a sample; the sample table and serial logs are the evidence.

## Baseline and after-fix expectation

At the K143 wave base the dynamic login is expected to be about 1.5-1.6 s per step, so `--check`
fails. This harness does not register a failing qtest in the default suite. After K139 lands, the
same command should show 5/5 (target 20/20) samples under 1 s per step; that after-fix run belongs
to the next wave, not to K143.

No donor code is involved; the method is a serial-console timing measurement.

## Files

* `scripts/login-cold-start.sh` — shell entry point.
* `tests/fixtures/login-cold-start/measure.py` — QEMU/pexpect harness and summary.
* `build/login-cold-start/` — generated images, per-sample transcripts and optional baseline JSON.
