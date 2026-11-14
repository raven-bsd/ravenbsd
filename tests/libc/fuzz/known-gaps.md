<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# Known gaps

## DEFERRED targets

| target | reason |
|---|---|
| `iconv_stream` | `DEFERRED: C48 not done` (`raven-libc-iconv` does not exist) |

## Not covered by this ticket

| target | owner |
|---|---|
| `malloc_ops` | C49 (`raven-libc-malloc`) |
| `qsort_cmp` | C41 (`raven-libc-stdlib`) |
| `locale_name` | C48 (locale) |

## Differential families

The oracle recipe list currently fails (`libc-oracle: recipe compat-crypt: flag -include is not
whitelisted`), so no `diff` feature is enabled and every differential column is `model` or
`none`:

| target | differential |
|---|---|
| printf_format | none (C43 float is the shell's) |
| scanf_format | none |
| strtod_input | model (shortest-digit round trip) |
| strtol_input | model (`model::int`) |
| time_format | none |
| tz_parse | none |
| regex_compile_exec | none |
| glob_pattern | none |
| resolver_message | none |
| resolver_conf | none |
| elf_dynamic | model (`model::elf_hash`, `model::gnu_hash`, init order) |
| utf8_decode | model (`core::str::from_utf8`) |
| inet_text | none |
| getopt_args | none |

## Harness deviations

* `P-tz-2` checks the documented `TzState` size budget instead of the transient allocation peak:
  parsing a TZif necessarily allocates its transition vectors, and the C44 item 11 bound is a
  state-size bound, not a whole-parse heap bound.
* `P-utf8-5` follows C39's `CODEUNIT` mapping (`0x80..=0xFF` -> `0xDF80..=0xDFFF`), not a
  literal byte value; the owning ticket wins over the shorthand in C70.
* `P-getopt-2` allows `optind` to decrease in permuting mode; only the non-permuting (POSIX or
  `+` prefix) scan must be monotone.
