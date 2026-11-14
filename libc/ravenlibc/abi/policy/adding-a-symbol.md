<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->
# Adding a symbol

This file is normative and follows `donor-map/14-headers-abi.md` 3.A.7 and 02 section 2.

1. Add the census/`assignments.toml` row with the ring and status; the reviewer owns the ring.
2. Put the name in the shell `Symbol.map` under the currently open node. Before the first release
   that is `RAVEN_1.0`; afterwards it is the newest unfrozen `RAVEN_1.n`. Never add to a frozen node.
3. Add the header declaration (C38) and the shell/`exports.toml` implementation.
4. Run `cargo xtask libc-abi gen --check`.
5. A data symbol also needs a `data-symbols.toml` row with its size and `verified = false`.
6. A new `RAVEN_1.n` node is created only by a release that adds public symbols, never per symbol;
   move the `RAVENprivate_1.0` parent to it.
