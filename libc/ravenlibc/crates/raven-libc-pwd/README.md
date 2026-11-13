<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-pwd

The `passwd`, `group` and shadow models and parsers, the `_r` buffer packing, `getgrouplist`,
the `initgroups` growth rule, the per-call plain-file source, the pwcache and `/etc/shells`
iteration.

The crate is `no_std` + `alloc`, class P (`#![forbid(unsafe_code)]`), and depends on
`raven-libc-abi` and `raven-libc-nss` (for `FileSource` and `ColonScanner`). User databases are
plain files read on every call; there is no `pwd.db`, nscd, YP or global state.

## Fuzz entry points

```
pub fn parse_passwd_line(line: &[u8]) -> Option<Passwd>
pub fn parse_group_line(line: &[u8]) -> Option<Group>
pub fn parse_shadow_line(line: &[u8]) -> Option<Spwd>
pub fn parse_master_passwd_line(line: &[u8]) -> Option<(Passwd, Spwd)>
pub fn pack_passwd(pw: &Passwd, buf: &mut [u8]) -> Result<PackedPasswd, PwError>
pub fn pack_group(gr: &Group, buf: &mut [u8]) -> Result<PackedGroup, PwError>
```

## Checks

```
cargo test -p raven-libc-pwd
cargo clippy -p raven-libc-pwd --all-targets -- -D warnings
cargo build -p raven-libc-pwd --target x86_64-unknown-none
```
