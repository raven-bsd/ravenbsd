<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K403 raven-pi4-platd host-link evidence

Tree `de0ac947` (K403 branch). The package is `tests/fixtures/pi4-platd` (the ticket's
`userland/drivers/pi4-platd` path is stale); its `pi4-platd-client` bin is an x86_64
`no_std`/`no_main` platform fixture.

| File | Result |
|---|---|
| `k403-host-repro.log` | `cargo test --no-run -p raven-pi4-platd` fails: duplicate `_start` (Scrt1.o + `raven_rt`) |
| `k403-host-fixed.log` | same command after `[[bin]] test = false`: exit 0, no host test target |
| `k403-real-target.log` | real fixture target `x86_64-unknown-none --release` with the multi-package set: linked `pi4-platd-client` |
| `k403-aarch64-bare.log` | ticket's `aarch64-unknown-none` target: not installed (`can't find crate for core`) |
| `k403-aarch64-softfloat.log` | `aarch64-unknown-none-softfloat`: no panic handler in a single-package build; the fixture is x86_64-only |

The platform qtest remains `tests/qtest/cases/pi4-platd.toml`; no platform test was deleted or
weakened. The real-target binary is unchanged.
