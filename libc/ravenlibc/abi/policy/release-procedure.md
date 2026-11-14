<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->
# Release procedure

This file is normative and follows `donor-map/14-headers-abi.md` 3.A.7.

1. Regenerate the census and require `complete = true`.
2. Run `cargo xtask libc-abi gen --check` until clean.
3. Triage ring 0/1 gaps; no class-A ring-0/1 gap may remain.
4. On a built `libc.so.1`, run `mkmap --check`, compare data sizes with `llvm-nm -D -S`, and set
   `verified = true`.
5. Run `cargo xtask libc-abi release --node RAVEN_1.0 --reviewer <name>`; it refuses on any
   incomplete condition and writes `status: released` plus `released_by`.
6. After release, `frozen-new` and every frozen difference are fatal.
