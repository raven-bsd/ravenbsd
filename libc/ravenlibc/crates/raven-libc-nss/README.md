<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-nss

The closed-set `nsswitch.conf` model: the six databases, the `files`/`dns`/skipped source set,
the `[NOTFOUND=return]` action, the shared `FileSource` seam and the bounded colon-line scanner.

The crate is `no_std` + `alloc`, class P (`#![forbid(unsafe_code)]`), and depends only on
`raven-libc-abi`. There are no plugins, no `dlopen`, no cache and no global state.

## Fuzz entry points

```
pub fn parse(file: &[u8]) -> Nsswitch
pub fn load(fs: &dyn FileSource) -> Nsswitch
pub fn dispatch<F: FnMut(Source) -> Status>(list: &SourceList, f: F) -> Status
pub fn ColonScanner::split_fields<'b>(line: &'b [u8], out: &mut [&'b [u8]]) -> usize
```

## Checks

```
cargo test -p raven-libc-nss
cargo clippy -p raven-libc-nss --all-targets -- -D warnings
cargo build -p raven-libc-nss --target x86_64-unknown-none
```
