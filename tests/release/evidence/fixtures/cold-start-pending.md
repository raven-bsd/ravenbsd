<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release notes (fixture, cold start pending)

### Dynamic cold start / interactive login — PENDING K139

In the release image, `spawn`→`main` takes **1.48 s** dynamically (76 ms static), and
`root`→`Password:` takes **1.6 s**. Command: `scripts/login-cold-start.sh` (K143) or
`cargo xtask qtest --case login-latency` (K139). Sample count: **0** post-fix samples;
acceptance 5/5 under 1 s per step, target 300 ms.

### Pi 4 board — UNVERIFIED
