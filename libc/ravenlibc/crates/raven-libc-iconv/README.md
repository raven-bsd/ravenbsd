<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-iconv

Pure-logic `iconv_open`/`iconv`/`iconv_close` of ravenlibc (donor-map
`02-ctype-locale-i18n.md` section 3.8). Ring 1 covers UTF-8, UTF-16/UCS-2, UTF-32/UCS-4,
`wchar_t`, US-ASCII and the musl legacy 8-bit charmaps; the optional `cjk` feature adds the
decode-first CJK channels (EUC-JP, Shift-JIS, ISO-2022-JP, GB18030/GBK/GB2312, Big5, EUC-KR).
`//TRANSLIT` and `//IGNORE` follow the musl outcome table.

The crate is `no_std` plus `alloc`, class P (`#![forbid(unsafe_code)]`); descriptors, `errno`
mapping and every `extern "C"` symbol live in the shell.

`src/tables_8bit.rs` is generated from `musl/src/locale/{codepages,legacychars}.h`
(`7813e9119eb5fe681b6750b8a033a0490bca0941`); regenerate with:

```
cargo run -q -p raven-libc-iconv --example gen_8bit -- <path>/musl/src/locale
cargo test -p raven-libc-iconv
cargo test -p raven-libc-iconv --features cjk
```
