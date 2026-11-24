<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# aarch64 conformance divergences

The S182 contract has no skipped rows. When a shared table cannot run on aarch64, add one
`<topic>.md` here and the ticket that will fix it. The runner still fails that table: the
divergence is visible evidence, not a green row and not a skip.

Each record names:

* the table path (`tests/conformance/<topic>/<name>.toml`);
* the exact aarch64 limitation and the failing row/expectation;
* the donor and POSIX/FreeBSD line that defines the contract;
* the owner and ticket that will remove the divergence.

The runner rejects a table that declares `skip =` before QEMU starts, so a table cannot quietly
turn a red aarch64 row into a skipped one.
