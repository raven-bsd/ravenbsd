<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-netcore

OS-independent socket layer over [smoltcp](https://github.com/smoltcp-rs/smoltcp). `netd` will use
it to own the interface, socket table and readiness state, while the host tests exercise the same
code through an in-memory Ethernet link.

The crate is `no_std` and needs only `alloc`. It forbids `unsafe`.

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

All operations are nonblocking:

* `send` returns `Ok(n)` or `Errno::EAGAIN` when the transmit buffer is full; TCP may accept a
  short write.
* `recv` returns `Ok(0)` only after the peer's orderly close; an empty live socket returns
  `Errno::EAGAIN`.
* `poll(now_ms)` drives smoltcp and returns readiness *transitions* since the previous call,
  not levels.
* `next_poll_delay_ms(now_ms)` reports the stack's next timer deadline. A link with its own
  propagation delay must still be polled at least as often as that delay.
* Unknown socket ids return `Errno::EBADF`. Wrong socket types return `Errno::ENOTSOCK` or
  `Errno::ENOTCONN` as appropriate.

Socket ids are allocated monotonically and are not reused until the `u32` counter wraps. A TCP
`close` invalidates the id immediately but keeps the smoltcp socket alive until the FIN handshake
finishes. A listener close stops new accepts; already completed, unaccepted connections keep their
own ids.

## Interface configuration

`set_ipv4(addr, prefix, gateway)` installs a static address (prefix lengths above 32 are clamped
to 32) and replaces any DHCP client. `enable_dhcp()` clears the static address and installs the
smoltcp DHCP client; lease changes are applied during `poll`.

Only IPv4 is exposed in this ticket. IPv6 can be added when the API is needed; the current
smoltcp feature set does not enable it.

## smoltcp features used

```text
alloc, medium-ethernet, proto-ipv4, socket-tcp, socket-udp, socket-dhcpv4
```

`socket-dhcpv4` pulls in `proto-dhcpv4`. IP fragmentation is not enabled, so UDP datagrams must
fit the link MTU (1500-byte IP MTU).

## Tests

```text
cargo test -p raven-netcore --features testutil
cargo clippy -p raven-netcore --all-targets --features testutil -- -D warnings
cargo build -p raven-netcore --target x86_64-unknown-none
```

`testutil` provides `PairDevice`: two endpoints connected by in-memory queues with configurable
loss, reordering, delay and jitter driven by a deterministic PRNG. The test harness advances time
manually and verifies TCP echo (including 1 MiB with 2% loss), orderly close, refused and
backlogged connections, `EAGAIN` transitions, UDP boundaries and source addresses, event
sequences, determinism and a large time jump.

The DHCP client is tested for state transitions only: this ticket has no in-test DHCP server. A
server would need a third endpoint or raw frame injection into the pair, which is outside the
OS-independent socket layer under test.
