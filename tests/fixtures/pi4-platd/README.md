<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# pi4-platd fixture

P12 fixture for the P8 platform launch path. The initfs installs `raven-init` (svcd), `devd`,
`platd` and a `plat:` client. The driver table maps the P8 fixture GENET node to
`/sbin/platd` with `fake`; `devd dtb launch` creates the platform BAR, transfers it and waits
while platd serves the deterministic backend. The client starts after `scheme:plat` and exercises
`plat:model`, `plat:mac`, `plat:clock/12`, `plat:power/0`, GPIO claim/conflict/config/release and
RNG reads.

The fixture reuses `tests/fixtures/pi4-devd/pi4-devd.dtb.hex` (read-only) and runs on x86_64.
It is a launch/scheme test, not a Pi 4 register test; the hardware capability plan is validated
by `cargo nextest run -p raven-platd` and the release profile edit is documented in
`docs/pi4-platd.md`.
