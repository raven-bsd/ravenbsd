# raven-libc-ctype

Pure-logic `<ctype.h>`, `<wctype.h>`, `wcwidth`/`wcswidth` and the UTF-8 multibyte codec of
ravenlibc (donor-map 02 sections 3.1-3.3). The crate is `no_std`, allocation-free and
`forbid(unsafe_code)`; locale state, `errno` and every `extern "C"` symbol live in the shell.

`src/unicode/tables.rs` is generated from `xtask/data/ucd-15.1.0` (Unicode 15.1.0) and must never be
edited by hand. Regenerate and check it with:

```
cargo run -q -p raven-libc-ctype --example gen_unicode -- \
  --ucd xtask/data/ucd-15.1.0 --out libc/ravenlibc/crates/raven-libc-ctype/src/unicode/tables.rs
cargo test -p raven-libc-ctype
```
