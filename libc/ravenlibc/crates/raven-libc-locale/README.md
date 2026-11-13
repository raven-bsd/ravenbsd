<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-locale

Pure-logic locale model and language services of ravenlibc (donor-map
`02-ctype-locale-i18n.md` sections 3.4-3.7): a per-category `locale_t` state with `setlocale`,
`newlocale`, `uselocale`, `duplocale`; `nl_langinfo` and `localeconv`; code-point collation,
wide strings and wide case comparison; GNU gettext (`.mo` lookup and the plural-expression
evaluator); and XSI `catopen`/`catgets` over the `gencat` format.

The crate is `no_std` plus `alloc`, class P (`#![forbid(unsafe_code)]`). Locale state is explicit;
the shell owns the global lock, the per-thread `locale_t` slot and every `extern "C"` symbol.
Locale names follow H-3: `C`/`POSIX` are single-byte, every other accepted name is UTF-8.

Run `cargo test -p raven-libc-locale`. Sources are ported from musl
`7813e9119eb5fe681b6750b8a033a0490bca0941`; the per-file provenance is in `PROVENANCE.toml`.
