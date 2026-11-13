<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# gdtoa shim

Replacement headers for the NetBSD gdtoa oracle family, written from scratch for C42:

* `namespace.h` - empty (no NetBSD namespace aliases in one merged object).
* `reentrant.h` - no-op mutex stand-ins (the recipe does not define `_REENTRANT`; the Rust port
  replaces both locks with the per-call arena). Tests that call the C original from several
  threads hold one Rust mutex around each call.
* `setlocale_local.h` - `locale_t`/`_current_locale()` plus `localeconv_l()` mapped to the host C
  locale's `localeconv()`; the oracle always runs in the C locale. It also defines `__UNCONST`,
  which NetBSD's `sys/cdefs.h` provides and the host header does not.
* `machine/ieee.h` - the little-endian double and x87 80-bit layouts `hdtoa.c` and `ldtoa.c` need,
  written from the amd64 psABI (not copied from the NetBSD header).
* `float.h` - takes the real host `<float.h>` through `include_next` and replaces the constant
  `FLT_ROUNDS 1` with the live `fegetround()` mapping that NetBSD's `float.h` provides; `hdtoa.c`
  switches on it.
* `locale.h` - host `<locale.h>` plus a `localeconv_l()` (the host C library has none) that returns
  the process locale's `struct lconv`, which is the C locale for every oracle run.


`<machine/ieee.h>` resolves through `-Ishim/gdtoa`, whose `machine/` subdirectory is searched before
any donor directory; no `include_next` is used.
