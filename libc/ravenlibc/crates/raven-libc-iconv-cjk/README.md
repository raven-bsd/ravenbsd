<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-iconv-cjk

Generated static CJK mapping tables for `raven-libc-iconv` (donor-map
`02-ctype-locale-i18n.md` section 3.8): JIS 0208, the reverse JIS index, GB18030 (two- and
four-byte), Big5, HKSCS, KSC 5601 and the GB18030/UTF pair list. The crate is `no_std`,
allocation-free, class P and has no dependencies; decode-first (H-8/RD-17) uses the forward
tables and encoding uses the reverse JIS index.

The tables are generated from `musl/src/locale/{jis0208,revjis,gb18030,gb18030utf,big5,hkscs,ksc}.h`
at `7813e9119eb5fe681b6750b8a033a0490bca0941`; never edit them by hand. Regenerate with:

```
cargo run -q -p raven-libc-iconv-cjk --example gen_tables -- <path>/musl/src/locale
cargo test -p raven-libc-iconv-cjk
```

The generated sources carry the musl MIT notice and a `RavenBSD: transpiled from` marker; the
per-file provenance is in `PROVENANCE.toml`.
