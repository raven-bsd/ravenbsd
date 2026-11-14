<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->
# The ABI freeze check

This file is normative. `cargo xtask libc-lint abi-freeze` runs the offline frozen checks and is
part of `make check`; `cargo xtask libc-abi check` runs the full audit.

Keys include `frozen-missing`, `frozen-version`, `frozen-kind`, `frozen-size`, `frozen-arch`,
`frozen-ring`, `frozen-new`, `list-mismatch`, `node-chain` and `data-unverified`. A missing frozen
symbol has no waiver: restore it or bump the major (an owner decision). A legitimate ring change
needs a `ring-changes.toml` row approved by the reviewer.
