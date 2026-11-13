# raven-libc-regex

<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

Spencer regular expressions for ravenlibc: the pure-logic port of the NetBSD
`lib/libc/regex` engine (`regcomp`, `regexec`, `regerror`, `regfree`, `regnsub`,
`regasub`), both matcher families and the UTF-8/byte locale seam.

`architecture/raven-libc/donor-map/07-regex-glob-fts.md` section 3.1 is the map; the
C-ABI shell owns `regex_t`, `regmatch_t`, the exported symbols and the thread locale.

## Layout

| File | Donor | What |
|---|---|---|
| `src/utils.rs` | `utils.h` | `NC`, `OUT`, `uch` |
| `src/cname.rs` | `cname.h` | the 95 `[.name.]` entries, mechanically converted |
| `src/regex2.rs` | `regex2.h` | strip opcodes, `cset`, `re_guts` |
| `src/regcomp.rs` | `regcomp.c` | the parser and strip builder |
| `src/engine.rs` | `engine.c` | one generic engine over `StateSet` |
| `src/regexec.rs` | `regexec.c` | the `smatcher`/`lmatcher`/`mmatcher` dispatcher |
| `src/regerror.rs` | `regerror.c` | messages, `REG_ATOI`/`REG_ITOA` |
| `src/regfree.rs` | `regfree.c` | `Drop for Regex`, the magic guard |
| `src/regsub.rs` | `regsub.c` | `regnsub`, `regasub` |
| `src/locale.rs` | first party | `RegexLocale`, `Utf8Locale`, `ByteLocale` |
| `src/consts.rs` | first party | flag values, error codes, limits |

`REG_STARTEND`, `REG_NOSPEC`, `REG_PEND`, `REG_GNU`, `REG_BACKR` and `REG_LARGE`
keep their BSD semantics. `REG_GNU` is off unless the caller passes `0o400`; `\<`,
`\>` and `[[:<:]]`/`[[:>:]]` work without it.

## Hardening on top of the donor

* `MAX_STRIP = 1 << 21` sops (H-RE-1): `enlarge`/`doemit`/`dupl` refuse a larger strip.
* `BACKREF_BUDGET = 1 << 24` calls per `regexec`, with `BACKREF_DEPTH_MAX = 4096`
  logical frames (H-RE-2); the logical recursion is an explicit heap stack, so a deep
  pattern returns `REG_ESPACE` without consuming the thread stack.
* `MAX_PAREN_DEPTH = 256` nested groups in the parser (H-RE-3).
* Every limit reports `REG_ESPACE`; the linear matchers never touch the budget.

## Tests

```
cargo test -p raven-libc-regex
cargo test --release -p raven-libc-regex
```

`tests/vectors.rs` runs all 19 NetBSD `.in` files and all 7 AT&T `.dat` files in the
byte and UTF-8 locales and prints the per-file counts; `tests/hardening.rs` covers the
limits and the ReDoS timings; `tests/exhaust.rs` re-implements the seven `t_exhaust.c`
shapes with a counting allocator; `tests/binary.rs` covers NULs and high bytes.

The vendored data under `tests/data/` is byte-identical to the donor (SHA-256 in the
C45 report). `t_regex_att.c` and `t_exhaust.c` are BSD-4 and were READ ONLY; the drivers
are written from the two READMEs and from the donor's behaviour.

## Spencer notice

The engine is derived from Henry Spencer's regular-expression package. The original
notice, reproduced from `lib/libc/regex/COPYRIGHT` of the donor, is:

```
Copyright 1992, 1993, 1994 Henry Spencer.  All rights reserved.
This software is not subject to any license of the American Telephone
and Telegraph Company or of the Regents of the University of California.

Permission is granted to anyone to use this software for any purpose on
any computer system, and to alter it and redistribute it, subject
to the following restrictions:

1. The author is not responsible for the consequences of use of this
   software, no matter how awful, even if they arise from flaws in it.

2. The origin of this software must not be misrepresented, either by
   explicit claim or by omission.  Since few users ever read sources,
   credits must appear in the documentation.

3. Altered versions must be plainly marked as such, and must not be
   misrepresented as being the original software.  Since few users
   ever read sources, credits must appear in the documentation.

4. This notice may not be removed or altered.

=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=
Copyright (c) 1994
	The Regents of the University of California.  All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:
1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the distribution.
3. Neither the name of the University nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
SUCH DAMAGE.
```

Each derived Rust file carries the upstream notice, the `RavenBSD: transpiled from`
line and the `Altered:` line that condition 3 requires. The shipped `NOTICE` should
carry the Spencer notice too (requested in the C45 report; `NOTICE` is outside this
ticket's ownership).
