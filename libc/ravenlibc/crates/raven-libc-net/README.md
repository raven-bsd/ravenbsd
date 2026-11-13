<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-net

The socket translation layer between the BSD socket API and Raven's `net:` scheme server
(donor-map `12-net-resolver-nss.md` sections 3.1-3.2 and 3.7). It owns the socket records, lazy
open fallback, option store, `sockaddr`/`cmsghdr` conversion, blocking deadlines, readiness
mapping, the resolver seams (`DnsIo`, `Probe`, `IfNames`) and `if_*`/`getifaddrs`.

`NetBackend`, `SocketFds`, `NetEnv` and `UnixBackend` are the seams. `FakeNetd` implements them
deterministically on the host; `backend_platform` and `fdglue` adapt C55. The crate is class P
(`#![forbid(unsafe_code)]`), `no_std` plus `alloc`, and has no clock, RNG or I/O outside a seam.

## Capability switches

`Caps` is all-false in "today" mode (the lazy-open fallback). PG-40 enables `bare_open`; PG-41
adds `udp_trunc_flag`, `udp_route_fail`, `accept_peer` and `peek_flag`; PG-42 adds `ifindex` and
`ipv6`. PG-43 supplies an optional `UnixBackend`; the default `NoUnix` returns `EAFNOSUPPORT`.
Interface indexes are the 1-based list position until PG-42, and that position is not stable
across interface changes.

## Checks

```
cargo test -p raven-libc-net
cargo clippy -p raven-libc-net --all-targets -- -D warnings
cargo build -p raven-libc-net --target x86_64-unknown-none
cargo build -p raven-libc-net --target aarch64-unknown-none-softfloat
cargo build -p raven-libc-net --target riscv64gc-unknown-none-elf
cargo xtask libc-lint
```

The on-target runs are C73 (`libc-net-loopback`, `libc-gai-dns`, `libc-ifaddrs`,
`libc-unix-pair`) and need PG-25/PG-40/PG-41/PG-42/PG-43.
