<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-flash fixtures

`fixtures/sys` is the shape used by the synthetic device-filter tests; the tests build an
equivalent tree programmatically so each case can add holders, symlinks and mounts without
sharing mutable state. `fixtures/mounts` is a small `/proc/mounts`-shaped sample.

The real-card write is owner evidence, not a CI test. No fixture is a real block device.
