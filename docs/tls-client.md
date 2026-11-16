<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TLS client: thin Rust wrapper over the OpenSSL add-on

Status: T2 implementation. `crates/raven-tls` is a small Rust API over the T1 OpenSSL 3.5 C
add-on. It owns no TLS state machine and implements no cryptography; the upstream `libssl` does
the handshake. Target evidence is the `tls-client` qtest, which performs a real handshake and a
byte exchange against a host `openssl s_server`.

## Boundary

`raven-tls` is an add-on consumer, not part of the C-free base scaffold:

* A target binary links the add-on's static `libssl.a`/`libcrypto.a` and the RavenBSD libc.
  Kernel, loader, system servers and base utilities do not link it and do not depend on this
  crate.
* The crate uses only upstream `libssl`/`libcrypto` entry points. There is no `rustls`, `ring`,
  `aws-lc`, custom crypto or hand-written TLS record/state code (DR-45).
* `unsafe` is confined to the FFI module; every block names the invariant the C call needs.
* Certificate-chain and hostname/IP verification are on by default. `connect` takes an explicit
  CA file and sets `SSL_VERIFY_PEER`; it never falls back to an unverified connection. DNS names
  set SNI plus `SSL_set1_host`; IPv4 literals set `X509_VERIFY_PARAM_set1_ip_asc`.

## API

```rust
pub trait Transport {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, TransportError>;
    fn write(&mut self, buffer: &[u8]) -> Result<usize, TransportError>;
}

pub fn connect(
    transport: Box<dyn Transport>,
    host: &str,
    ca_file: &str,
) -> Result<Connection, Error>;

impl Connection {
    pub fn write(&mut self, bytes: &[u8]) -> Result<usize, Error>;
    pub fn read(&mut self, bytes: &mut [u8]) -> Result<usize, Error>;
}
```

`connect` installs the caller's connected byte stream as a custom OpenSSL BIO, loads the
supplied CA bundle, performs the handshake and returns only when the peer certificate chain and
the expected host/IP verify. `host` is used for SNI and name verification; the transport is
already connected. The caller must run it on a thread that may block in the transport (the qtest
client is a one-shot service). `Connection` frees the `SSL`, its BIO, the BIO method, the
`SSL_CTX` and the transport on drop.

The transport seam exists because the target network path is the `net:` scheme: the libc
lazy-socket path currently reports `ENOSYS` for a pending OFD, so the client opens
`net:tcp/10.0.2.2:24443`, issues `CALL CONNECT` and hands the scheme stream to the wrapper. The
wrapper itself is transport-agnostic and has no dependencies outside `alloc`.

The crate is `no_std` outside host tests. Host tests cover the non-FFI helpers (`tests` module in
`lib_ut.rs`); the C path is exercised by the target qtest.

## Build and test

The target client and its initfs overlay are built by:

```sh
sh tests/tls/client/build.sh
```

The script builds the T1 add-on if its artifacts are absent, generates a test CA and a server
certificate with SAN `IP:10.0.2.2`, builds `tests/tls/client` for `x86_64-unknown-ravenbsd`
against `build/tls/x86_64`, and packs a `server`-profile initfs overlay with the client, the CA,
the one-shot service manifest and a `/dev/urandom` mount backed by `rand:`.

The guest realtime clock is zero until an RTC source is wired into the kernel, so the test
certificates are issued with `notBefore` 1970-01-01 and `notAfter` 2049-12-31. Chain and
hostname verification remain enabled; the dates only match the guest clock.

```sh
cargo test -p raven-tls
cargo xtask qtest --case tls-client --arch x86_64
```

The qtest boots the normal `server` profile, adds a QEMU user-mode virtio-net NIC, and starts
`openssl s_server -no_ticket -www -naccept 1` on the host before QEMU. The client and the server
negotiate TLS 1.3. Session tickets are disabled on the test server only: with tickets enabled,
the post-handshake ticket flight exposes a netd/TCP retransmission race that duplicates the
client's compatibility ChangeCipherSpec and makes `s_server` abort with `unexpected_message`.
The add-on and the wrapper do not disable tickets; the TCP bug is tracked as a platform issue.
The expected transcript is:

```
raven-tls: connect 10.0.2.2:24443
raven-tls: handshake ok
raven-tls: wrote 34 bytes
raven-tls: read <n> bytes
raven-tls: client ok
```

The one-shot service has `needs = ["scheme:net"]`, so netd's `scheme:net` readiness gates the
client. The test-only `/etc/svcd.conf` exits init with the client status.

## Entropy

OpenSSL's `linux-x86_64` configuration uses the `DEVRANDOM` seeding path when built for
RavenBSD (no host `__linux`). The `server` profile does not mount `/dev/urandom`; the test
overlay maps it to the `rand:` scheme, which is also the source behind `getentropy`. The current
`rand:` implementation is explicitly not cryptographically secure until a real entropy source
and the PG-52 `getrandom` path land. This is a platform risk, not a reason to reimplement TLS:
the production fix is to wire the entropy source and the mount, not to replace OpenSSL.

## Update and security tracking

The pinned OpenSSL version, provenance and CVE/update procedure are in `docs/tls.md`. This
wrapper adds no crypto code, so an OpenSSL update is a T1 add-on update plus a re-run of
`cargo test -p raven-tls` and the `tls-client` qtest. The host `openssl s_server` used by the
qtest must support TLS 1.2/1.3 (OpenSSL 3.x or a compatible build).

## Limitations

* IPv4 literals only; there is no DNS resolution in the wrapper and no IPv6 socket path.
* The transport is blocking; the target client retries `EAGAIN` on the `net:` stream. There is
  no non-blocking/poll integration yet.
* The target test uses a custom BIO over `raven-scheme`; the POSIX socket path is not used
  because libc cannot install a pending socket OFD. A future libc fix can add a socket-backed
  transport without changing the TLS wrapper.
* OpenSSL is built `no-threads` in T1; the client is single-threaded. A future multi-connection
  consumer needs the pthread build or the scheme wait path.
* The compat shim's `select`/`pipe` stubs from T1 are unused by this blocking client path.
* The TLS 1.3 session-ticket path is not exercised by the qtest until the netd/TCP retransmission
  race is fixed; the library itself still offers tickets.
