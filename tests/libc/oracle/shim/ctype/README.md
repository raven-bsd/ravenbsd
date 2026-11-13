<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# ctype shim

`locale_impl.h` is the minimal replacement for musl's private header of that name. It exposes a
single settable weak global, `mb_cur_max`, so the differential test can run the same compiled
donor code at `MB_CUR_MAX` 1 (C/POSIX) and 4 (UTF-8). Written from scratch (guide 6.2); no donor
text is copied.
