<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P48 - POSIX iconv, gencat and locale

## Tool matrix

| Tool | Supported options | Behaviour | Evidence |
|---|---|---|---|
| `iconv` | `-f from`, `-t to`, `-o file` | convert UTF-8, ASCII, ISO-8859-1; stdin/stdout | `test.sh`; `lib_ut` |
| `gencat` | positional `catalog source...` | compile `$set`/message syntax to the runtime catalog image | `test.sh`; `lib_ut` |
| `locale` | `-a`, `-m`, `-k`, `[category...]` | report `LANG`/`LC_*`, codeset and known names | `test.sh`; `lib_ut` |

## Conversion matrix

| From | To | Behaviour |
|---|---|---|
| UTF-8 | ASCII | ASCII only; a code point above U+007F fails |
| UTF-8 | ISO-8859-1 | code points up to U+00FF; above fails |
| ASCII | UTF-8 / ISO-8859-1 | byte-to-character, always representable |
| ISO-8859-1 | UTF-8 / ASCII | Latin-1 to Unicode; ASCII fails above U+007F |
| any | same | revalidates the input |

Accepted aliases: `UTF-8`/`UTF8`, `ASCII`/`US-ASCII`/`ANSI_X3.4-1968`,
`ISO-8859-1`/`ISO8859-1`/`LATIN1`/`LATIN-1`. Conversion is whole-input: a failure produces no
partial output. Exit status is 0 on success, 1 for an unsupported encoding or conversion failure,
2 for usage.

## Catalog matrix

`gencat` supports `$set N`, `$delset N`, `$transient`, `$comment` and `<msgid> "<string>"`.
Strings are UTF-8 and support `\\`, `\"`, `\n`, `\t` and `\r`. Set and message ids must be
positive. Multiple source files share the current set. The output is the runtime image read by
`raven_libc_locale::nls::Catalog` (magic `ff88ff89`).

The compiler replaces the output catalog; it does not merge an existing one. The maximum image
is 8 MiB, matching the runtime reader.

## Locale matrix

`locale` resolves the environment through `LocaleState::setlocale(LC_ALL, "")` and reports the
six categories plus `codeset` from `nl_langinfo(CODESET)`. An unset environment falls back to
`C.UTF-8`. `-a` lists `C`, `POSIX`, `C.UTF-8`; `-m` lists `ANSI_X3.4-1968` and `UTF-8`; `-k`
lists `LANG`, the six `LC_*` names, `LC_ALL` and `codeset`. A category operand prints one
`NAME="value"` line. Exit status is 0 on success, 1 for an unknown category, 2 for usage.

## Exact unsupported cases

- `iconv`: UTF-16, UTF-32, EUC, Shift-JIS, ISO-2022, stateful encodings, transliteration,
  `-c`, `-s` and `//TRANSLIT`/`//IGNORE` suffixes.
- `iconv`: streaming/incremental conversion; the whole input is buffered.
- `gencat`: merging an existing catalog, multi-line messages, adjacent string concatenation,
  octal/hex escapes, symbolic set names, `$codeset` and `-` source stdin.
- `locale`: locale archive discovery beyond the three built-in names, `-c`, `-u`, `-v`,
  `LC_COLLATE`/`LC_TIME` data beyond the C/C.UTF-8 tables, and target execution (host-only
  binaries).

## Acceptance

```
sh tests/posix/utility/run-locale.sh
cargo nextest run -p iconv -p gencat -p locale
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

Evidence: `docs/posix/evidence/P4-locale.toml`.
