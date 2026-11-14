<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->
# libc.so.1 soname compatibility

This file is normative and follows `donor-map/14-headers-abi.md` 3.A.8.

* `DT_SONAME` is `libc.so.1` for major 1 (nodes `RAVEN_1.x`).
* Every dynamic program has exactly one Raven library in `DT_NEEDED`: `libc.so.1`.
* `libc.so.2`/`RAVEN_2.x` only when a frozen symbol must go, a data size/layout changes, or the
  loader contract changes; it is an owner decision.
* Minor nodes rise per release that adds public symbols and chain to their predecessor.
  `RAVENprivate_1.0` is last and never incremented.
* No `libc.so.1.N` files or `libc.so.6`; the loader's alias table covers the ring-2 names.
* `.note.raven.abi` revision rises only with a new major.
