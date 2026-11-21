<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K408 post-exec socket write fixture

In-guest isolation of the scheme-socket write path that K195 suspects after the SSH session's
fork+exec. The fixture runs in seconds and covers the five variants that matter:

| Variant | Guest marker | Shape |
|---|---|---|
| same process | `SAME` | write on the connected socket in the parent |
| after fork | `FORK` | forked child inherits the socket and writes |
| after fork+exec | `EXEC` | child receives the socket as fd 7 and writes after `execv` |
| after a read | `READ` | read the host's `READ` trigger on the same fd, then write |
| second write | `SECOND` | write only after the host acknowledges with `ACK2` |

The host peer verifies every marker; the `READ`/`ACK2`/`DONE` triggers are sent only after the
earlier markers arrive, so a missing variant blocks the guest at the corresponding serial line.

## Run

```sh
sh tests/fixtures/socket-write-exec/build.sh
cargo xtask qtest --case socket-write-exec --arch x86_64
sh tests/fixtures/socket-write-exec/run-tap.sh      # accepted-socket shape over the K194 tap
```

The fixture overlays two static C binaries (`/bin/socket-write-exec`,
`/bin/socket-write-exec-child`) and a one-shot service on the existing `server` profile (virtio-net,
DHCP, netd). The host peer is `tests/fixtures/socket-write-exec/host.py`; it listens on
`0.0.0.0:23096` and the guest connects to `10.0.2.2:23096` through QEMU user networking.

Artifacts: `build/fixtures/socket-write-exec/host.log` (verified markers),
`build/fixtures/socket-write-exec/qemu.pcap` (netdev capture), and the qtest serial log.

## Result on the merged tree

`cargo xtask qtest --case socket-write-exec`: **pass**. The guest serial:

```text
socket-write-exec: connected fd=0
socket-write-exec: same sent
socket-write-exec: fork sent
socket-write-exec: exec sent
socket-write-exec: read request
socket-write-exec: read sent
socket-write-exec: second sent
socket-write-exec: host done
socket-write-exec: done
```

The host log shows `same=b'SAME'`, `fork=b'FORK'`, `exec=b'EXEC'`, `read=b'READ'`,
`second=b'SECOND'`, `verified all variants`. The generic post-exec write path is healthy: the
fork+exec child writes through the inherited connected socket, and a read-then-write works.

## Accepted-socket over the tap (K195 shape)

The first revision used the K195 shape through `hostfwd` and was confounded by the proxy: QEMU's
slirp closed the first host connection immediately after the TCP handshake, so the one-shot guest
accepted a stale connection, wrote `SAME`/`FORK`/`EXEC` into the closed peer, and blocked on the
`READ` trigger. The pcap shows the host-side FIN right after the handshake and a later RST. That
is the hostfwd path, not a write variant.

The accepted shape now runs on the **K194 tap** (`run-tap.sh` brings up `ravenbr0`/`ravendemo0`,
runs the qtest, and tears it down):

```sh
sh tests/fixtures/socket-write-exec/run-tap.sh
```

The guest owns the listener; the host peer connects to `10.0.2.15:23096` and verifies the same
five markers. Result: **pass**.

```text
socket-write-exec: listening port=23096
socket-write-exec: accepted fd=1
socket-write-exec: same sent
socket-write-exec: fork sent
socket-write-exec: exec sent
socket-write-exec: read request
socket-write-exec: read sent
socket-write-exec: second sent
socket-write-exec: host done
socket-write-exec: done
```

Host log: `same=b'SAME'`, `fork=b'FORK'`, `exec=b'EXEC'`, `read=b'READ'`,
`second=b'SECOND'`, `verified all variants`.

## Conclusion for K195

The generic scheme-socket write path is healthy in both shapes: connected socket (guest → host)
and accepted socket (host → guest over the tap), including a forked child, a fork+exec child on
the inherited fd, a read-then-write, and a second write after the host acknowledges. The remaining
K195 delta is therefore **OpenSSH-specific**, not the fd/exec/socket plumbing: the session child's
channel output is composed in `serverloop.c` (`channel_output_poll`/`server_loop2`, with the
confirmation recorded around `serverloop.c:650-660`) and held in the `channels.c` output buffer;
that POLLOUT-gated flush is the next trace for K195/K409. No product fix is made here.

## Limits

* One connection, one client; no retries in the guest (the host listener starts before QEMU).
* The fixture is x86_64 and test-only; nothing is staged in a product image.
* `tcp-accept-exec` is not present in this checkout, so the K408 acceptance cannot run it here.
