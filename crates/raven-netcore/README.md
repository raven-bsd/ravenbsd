<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-netcore

OS-independent socket layer over the native [`raven-tcpip`](../raven-tcpip) stack. `netd` uses it
to own the interface, socket table and readiness state, while the host tests exercise the same
code through an in-memory Ethernet link.

The crate is `no_std` and needs only `alloc`. It forbids `unsafe`. smoltcp, the stack this layer
was first written against, was retired in C32: there is one backend, `RavenTcpipBackend`, and
`NetCore<D>` is `Core<RavenTcpipBackend<D>>`. `StackBackend` stays as the seam for stubs and
future transports.

## Public API

```rust
pub struct NetCore<D: raven_netcore::Device> { /* ... */ }
pub struct SocketId(pub u32);

pub enum Event {
    Readable(SocketId),
    Writable(SocketId),
    Closed(SocketId),
    Error(SocketId, Errno),
    Accepted { listener: SocketId, conn: SocketId },
}
```

`Device` is `raven_tcpip::Device` (re-exported with `DeviceCaps`, `Instant`, `Medium`,
`ChecksumOffload` and `TxError`): `receive(now, consume)` hands the next frame to a callback and
`transmit(now, len, fill)` lets the stack write a frame in place.

All operations are nonblocking:

* `send` returns `Ok(n)` or `Errno::EAGAIN` when the transmit buffer is full; TCP may accept a
  short write. On a connection the peer has already torn down it returns `Errno::ENOTCONN`.
* `recv` returns `Ok(0)` only after the peer's orderly close; an empty live socket returns
  `Errno::EAGAIN`; a reset connection reports its error once.
* `poll(now_ms)` drives the stack and returns readiness *transitions* since the previous call,
  not levels.
* `next_poll_delay_ms(now_ms)` reports the stack's next timer deadline, or `None` when nothing is
  scheduled. A link with its own propagation delay must still be polled at least as often as
  that delay.
* Unknown socket ids return `Errno::EBADF`. Wrong socket types return `Errno::ENOTSOCK` or
  `Errno::ENOTCONN` as appropriate.

Socket ids are allocated monotonically and are not reused until the `u32` counter wraps. A TCP
`close` invalidates the id immediately; the connection finishes its FIN handshake and TIME-WAIT
inside the stack, and its table slot is reclaimed as soon as the application has let go, so a
server can accept far more than `max_sockets` connections over its lifetime. A listener close
stops new accepts; already completed, unaccepted connections keep their own ids.

## Interface configuration

`set_ipv4(addr, prefix, gateway)` installs a static address and stops any DHCP client.
`enable_dhcp()` / `try_enable_dhcp()` start the native DHCPv4 client (`ENETUNREACH` if the link
has no hardware address): it discovers, requests, renews at T1, rebinds at T2 and drops the
address when the lease ends; `dhcp_lease()` reports the lease in force. Lease changes are
applied during `poll`.

Only IPv4 is exposed in this layer; the native stack also speaks IPv6 and the API can grow when
`netd` needs it.

## Tests

```text
cargo test -p raven-netcore --features testutil
cargo clippy -p raven-netcore --all-targets --features testutil -- -D warnings
cargo build -p raven-netcore --target x86_64-unknown-none
```

`testutil` provides `PairDevice`: two endpoints connected by in-memory queues with configurable
loss, reordering, delay and jitter driven by a deterministic PRNG. `tests/scenarios.rs` covers
byte-exact transfers (1 MiB reliable, lossy and reordering links, both directions at once),
pinned readiness-event sequences, orderly close, refused connections, 150 sequential
connections through a 64-slot table, simultaneous connections, a full backlog, UDP boundaries,
DHCP on the wire and determinism from the seed. The wire-level DHCP exchange against a scripted
server lives in `raven-tcpip`'s `tests/dhcp.rs`.
