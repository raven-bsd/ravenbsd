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

K135 measured the dynamic login at about 1.5-1.6 s per step on its pre-S129 tree. The available
pre-base `e22c15df` release gate image now measures about **155 ms median** (p90 294 ms) for
`login:` -> `Password:` and **127 ms median** (p90 228 ms) for `Password:` -> `raven# `, so
`--check` passes on that image. The exact wave-base image build was blocked by host load; rerun the
harness at the base before treating the regression as fixed. This harness does not register a
failing qtest in the default suite. The after-fix 20/20 run belongs to the next wave, not K143.

No donor code is involved; the method is a serial-console timing measurement.

## K149 after-fix matrix: BLOCKED on host load (2026-11-21)

K149 requires the after-fix matrix on an **idle** host: 20/20 TCG4 and 20/20 KVM2
samples with each step's median and p90 under 1 s (target 300 ms). The harness and
the release image are ready, but the host was not idle for the whole window
(1-min load 112–171 on 20 cores; other lanes were building/booting), so neither
matrix completed inside its ticket cap. Loaded-host samples are not an idle PASS.

Image: `build/login-cold-start/release.img`, commit `978899da`,
sha256 `e18870b9ff3c5ff0d870b805881659215a61d05cac18cd4f82ba62c5c516d687`.

| Config | Command (cap) | Result |
|---|---|---|
| TCG `-smp 4` | `timeout 300 sh scripts/login-cold-start.sh --accel tcg --smp 4 --samples 20` | BLOCKED: exit 124 (300 s) at sample 6/20, load 121–171 |
| KVM `-smp 2` | `timeout 240 sh scripts/login-cold-start.sh --accel kvm --smp 2 --samples 20` | BLOCKED: exit 124 (240 s) at sample 16/20, load 62–82 |

Partial data (loaded host, **not** an idle verdict):

* KVM2 15/20 samples ok: `login:`→`Password:` 58–63 ms, password→prompt
  150–363 ms.
* TCG4 6 attempts: 2 ok (`login:` 371/627 ms, password 8731/14822 ms),
  4 harness timeouts/EOF.

Per-sample serial logs are under `build/login-cold-start/<UTC>/`. The idle rerun
is the remaining acceptance step; `scripts/login-cold-start.sh` and the image can
be reused as-is.

## Files

* `scripts/login-cold-start.sh` — shell entry point.
* `tests/fixtures/login-cold-start/measure.py` — QEMU/pexpect harness and summary.
* `build/login-cold-start/` — generated images, per-sample transcripts and optional baseline JSON.
