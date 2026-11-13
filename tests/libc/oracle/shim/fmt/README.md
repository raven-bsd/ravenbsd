<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# fmt shim

Replacement headers for the `fmt-freebsd` and `fmt-musl*` oracle families, written from
scratch (transpile guide 6.2). Nothing here is copied from a donor.

* `stdio.h` - the private FreeBSD `FILE` image (`struct _IO_FILE` with the `_bf`/`_p`/`_w`
  string-stream fields, `FAKE_FILE`, the `__S*` flags), for the FreeBSD printf family. The
  host `<wchar.h>` sees the same `struct _IO_FILE`, so its `FILE *` declarations agree.
* `xlocale_private.h` - `__get_locale`, `FIX_LOCALE`, `__strerror_rl`, the `_l` aliases and
  the `%m` text used by both the C and the Rust side (`errno N`).
* `locale.h` - the host C locale plus a `localeconv_l` whose `LC_NUMERIC` data is
  `thousands_sep = ","` and `grouping = "\3\3"`; the `'` flag cells compare against it.
* `libc_private.h`, `namespace.h`, `un-namespace.h` - empty: the oracle links one merged
  object, so the libc-internal symbol renaming and locking are not used.
* `printf.h`, `xlocale.h` - empty replacements for headers the donor includes but whose
  machinery (`register_printf_*`, non-`_l` locale entry points) is not built.
* `math.h`, `float.h` - host headers plus `fmt_glue.h`, which is included after `local.h`
  and `fvwrite.h` and therefore can define the weak string-stream helpers
  (`__sfvwrite`, `__swsetup`, `__fflush`, ...).
* `musl/` - the musl printf/scanf family: a forward-declaring `stdio.h` (musl defines
  `struct _IO_FILE` itself), `syscall.h`, `locale_impl.h`, `string.h`/`stdint.h`/`stdlib.h`
  wrappers that pull in `musl_glue.h` after `stdio_impl.h`, and the weak
  `__fwritex`/`__towrite`/`__toread`/`__uflow`/`__overflow`/lock helpers.

Helper policy: the FreeBSD recipe uses the NetBSD gdtoa objects from `recipes/fmt-freebsd`
for `__dtoa`/`__ldtoa`/`__hdtoa`/`__hldtoa`, so the digits are the same C42 engine on both
sides; `localeconv_l`, `__strerror_rl` and `__sfvwrite` come from this shim. The musl recipe
takes everything from musl except `wcrtomb`/`mbrtowc`/`wcsrtombs`/`strerror`/`__ctype_*`
from the host; `%m` in musl uses the host `strerror`.
