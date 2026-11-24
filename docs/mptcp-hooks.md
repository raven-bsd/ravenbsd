<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# MPTCP subflow hooks (MPTCP1b)

MPTCP (RFC 8684) carries data over ordinary TCP subflows. `raven-tcpip` exposes a small,
object-safe surface for the `raven-mptcp` core to create and steer subflows without reaching into
the socket table. No MPTCP wire logic lives in the TCP stack; every hook delegates to the existing
plain-TCP path.

## API

```rust
pub trait SubflowOps {
    fn open_subflow(&mut self, remote: (IpAddr, u16), local_if: Option<Ipv4Addr>)
        -> Result<SubflowId, Errno>;
    fn send_on(&mut self, subflow: SubflowId, data: &[u8]) -> Result<usize, Errno>;
    fn recv_from(&mut self, subflow: SubflowId, buffer: &mut [u8]) -> Result<usize, Errno>;
    fn readiness(&self, subflow: SubflowId) -> Readiness;
    fn close_subflow(&mut self, subflow: SubflowId);
    fn set_backup(&mut self, subflow: SubflowId, backup: bool);
    fn is_backup(&self, subflow: SubflowId) -> bool;
}
```

`Stack<D>` implements the trait; the MPTCP core can hold `&mut dyn SubflowOps`.

* `open_subflow` creates one TCP connection to `remote`. The current stack is single-homed:
  `local_if` may be `None` (use the configured IPv4 address) or the configured address itself; any
  other address returns `EADDRNOTAVAIL`. The chosen address is available through
  `Stack::subflow_local_if`.
* `send_on`/`recv_from` are the ordinary non-blocking `send`/`recv` on that subflow's socket; the
  same `EAGAIN` and `Ok(0)` (orderly EOF) semantics apply.
* `close_subflow` closes the plain TCP connection and keeps the handle so readiness can report the
  terminal state. Further `send_on`/`recv_from` on it returns `EBADF`.
* `set_backup`/`is_backup` store the MPTCP backup-path flag; the TCP stack never acts on it.

## Live readiness

`readiness` calls the same socket predicates as `Stack::readiness` on **every** call. It does not
read a latch or a remembered mask: a caller that aggregates subflows cannot be told that a drained
socket is still readable or that a full queue is still writable. `Readiness` carries
`readable`, `writable`, `hup` and `error`; a closed or unknown handle reports `hup + error`, and a
plain half-close remains readable until drained.

## Tests

`crates/raven-tcpip/tests/mptcp_hooks.rs` uses two plain TCP connections to a local listener:

1. open two subflows, steer `alpha`/`beta`, and verify each peer receives the right payload;
2. verify the backup flag round-trips;
3. peer-write → `readable`, drain → not readable (the live, no-cache check);
4. fill a subflow's send queue → not writable, drain → writable;
5. close the second subflow → `hup`, not writable, `send_on` → `EBADF`, while the first still
   carries data;
6. a wrong `local_if` is refused.

The negative check sets `MPTCP_HOOKS_STALE_READINESS=1`, which enables a test-only cached-readiness
seam. The live assertion (`new bytes must be readable`) then fails, proving the test catches a
remembered mask. The seam is compiled only for tests/`testutil` and is not part of the production
path.

```sh
cargo nextest run -p raven-tcpip --test mptcp_hooks
MPTCP_HOOKS_STALE_READINESS=1 cargo nextest run -p raven-tcpip --test mptcp_hooks  # must fail
```
