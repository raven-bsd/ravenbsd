<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-resolv

The pure-logic half of the RavenBSD resolver:

* the DNS wire codec: name compression and decompression, query construction, the answer
  parser, the question check and the EDNS0 OPT record;
* the multi-query UDP/TCP engine (`res_msend`), with one fresh UDP socket per lookup, a total
  timeout, SERVFAIL retries, the TC to TCP fallback and a resumable TCP state;
* `getaddrinfo` and `getnameinfo` with the musl search-list rules, the RFC 6724 destination
  sort and RFC 6761 `localhost`;
* the `inet_*` family, the byte-order helpers and the IPv6 constants;
* the `/etc/resolv.conf`, `/etc/hosts`, `/etc/services`, `/etc/protocols` and `/etc/networks`
  parsers, the iterating `servent`/`protoent`/`netent` states and the legacy `gethostby*` and
  `res_*` wrappers.

Everything outside the process goes through six seams from `src/traits.rs`: `FileSource`
(`raven-libc-nss`), `DnsIo`, `Clock`, `Entropy`, `Probe` and `IfNames`. There is no global
resolver state, no cache, no `unsafe`, no `std` outside tests, no hard-coded source port and no
clock-derived query ID. The crate is class P (`#![forbid(unsafe_code)]`) and `no_std + alloc`.

## Limits

`MAXNS` 3, `MAXADDRS` 48, `MAXSERVS` 2; DNS names at most 253 text bytes (254 with the dot) and
labels at most 63; `ABUF_SIZE` 4800; UDP queries at most 512 bytes and TCP messages at most
65535; `resolv.conf` lines at most 256, hosts lines at most 512 and
services/protocols/networks lines at most 128. An over-long line is skipped whole (D-12-3,
A-net-10).

## Fuzz entry points

Every function below is `pub`, takes only slices and trait objects, is panic-free on arbitrary
bytes, keeps its output bounded and terminates in a bounded number of steps. The deterministic
mutation corpus is `tests/corpus_dns.rs`; the C70 fuzz targets are built outside this crate.

```
pub fn dn_expand(msg: &[u8], pos: usize, out: &mut [u8; 256]) -> Option<(usize, usize)>
pub fn dn_comp(name: &[u8], dst: &mut [u8], ptrs: &mut [usize], last: Option<usize>) -> Option<usize>
pub fn dn_skipname(msg: &[u8], pos: usize) -> Option<usize>
pub fn is_valid_hostname(host: &str) -> bool

pub fn res_mkquery(op: u8, name: &[u8], class: u16, rtype: u16, id: u16, buf: &mut [u8]) -> Option<usize>
pub fn dns_parse<F: FnMut(u8, &[u8], &[u8]) -> i32>(packet: &[u8], cb: F) -> i32
pub fn dns_parse_offset<F: FnMut(u8, &[u8], &[u8], usize) -> i32>(packet: &[u8], cb: F) -> i32
pub fn question_matches(query: &[u8], answer: &[u8]) -> bool
pub fn pack_edns0(udp_size: u16, do_bit: bool, out: &mut [u8]) -> Option<usize>
pub fn append_edns0(buf: &mut [u8], len: usize, enabled: bool) -> Option<usize>

pub fn ns_get16(cp: &[u8]) -> Option<u16>
pub fn ns_get32(cp: &[u8]) -> Option<u32>
pub fn ns_put16(value: u16, cp: &mut [u8]) -> bool
pub fn ns_put32(value: u32, cp: &mut [u8]) -> bool
pub fn ns_initparse(msg: &[u8]) -> Option<NsMsg<'_>>
pub fn ns_skiprr(msg: &[u8], ptr: usize, eom: usize, section: i32, count: i32) -> Option<usize>
pub fn ns_parserr<'a>(handle: &mut NsMsg<'a>, section: i32, rrnum: i32) -> Option<NsRr<'a>>
pub fn ns_name_uncompress(msg: &[u8], eom: usize, src: usize, dst: &mut [u8; 256]) -> Option<(usize, usize)>

pub fn inet_aton(s: &[u8]) -> Option<[u8; 4]>
pub fn inet_pton(af: i32, s: &[u8], out: &mut [u8]) -> i32
pub fn inet_ntop(af: i32, a: &[u8], out: &mut [u8]) -> Result<usize, Errno>

pub fn lookup_ipliteral(name: &[u8], family: i32, ifnames: &dyn IfNames) -> Result<Option<Literal>, Eai>
pub fn getaddrinfo(host: Option<&str>, serv: Option<&str>, hints: &Hints, env: &Env, order: &HostOrder) -> Result<Vec<AddrInfoRec>, Eai>
pub fn getnameinfo(addr: &SockAddr, hostlen: usize, servlen: usize, flags: i32, env: &Env, order: &HostOrder) -> Result<(Option<String>, Option<String>), Eai>
pub fn gai_strerror(code: i32) -> &'static str

pub fn parse_resolv_conf(file: Option<&[u8]>) -> ResolvConf
pub fn parse_options_ext(file: &[u8]) -> ConfExt
pub fn parse_hosts_line(line: &[u8], ifnames: &dyn IfNames) -> Option<HostLine>
pub fn parse_hosts(file: &[u8], ifnames: &dyn IfNames) -> Vec<HostLine>
pub fn parse_serv_line(line: &[u8]) -> Option<Servent>
pub fn parse_protocols(file: &[u8]) -> Vec<Protoent>
pub fn parse_net_line(line: &[u8]) -> Option<Netent>
```

`msend` takes the seam traits and is fuzzed through `tests/common/fake_dns.rs`:

```
pub fn msend(env: &Env, conf: &ResolvConf, ext: &ConfExt,
             queries: &[&[u8]], answers: &mut [&mut [u8]], alens: &mut [usize])
    -> Result<(), Errno>
```

The ticket's corpus list also names `parse_passwd_line`, `parse_group_line` and
`parse_master_passwd_line`; those live in `raven-libc-pwd` and are covered by that crate's own
corpus (`C70` targets each crate). They are not re-exported here.

## Checks

```
cargo test -p raven-libc-resolv
cargo test -p raven-libc-resolv --test e2e_fake_server
cargo test -p raven-libc-resolv --test corpus_dns
cargo test -p raven-libc-resolv --test donor_expectations
cargo clippy -p raven-libc-resolv --all-targets -- -D warnings
cargo build -p raven-libc-resolv --target x86_64-unknown-none
cargo doc -p raven-libc-resolv --no-deps
```
