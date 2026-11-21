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

## Accepted-socket note for K195

The first revision used the K195 shape directly (guest listener + `hostfwd`). It did **not** reach
the variants: QEMU's hostfwd/slirp closed the first host connection immediately after the TCP
handshake, the one-shot guest accepted that stale connection, wrote `SAME`/`FORK`/`EXEC` into the
closed peer, and then blocked on the `READ` trigger. The pcap shows the host-side FIN right after
the handshake and a later RST:

```text
10.0.2.2.34896 > 10.0.2.15.23096: Flags [S], ...
10.0.2.15.23096 > 10.0.2.2.34896: Flags [S.], ...
10.0.2.2.34896 > 10.0.2.15.23096: Flags [F.], ...
...
10.0.2.2.34896 > 10.0.2.15.23096: Flags [R.], ...
```

That is the hostfwd/slirp path, not a failing write variant. The connected-socket fixture removes
that proxy and proves the five writes. The **accepted** fd shape (K195's sshd session) still needs
the K194 tap: the next K195 run should use the same five markers over the tap, with the host
connecting to `10.0.2.15:23096`, to separate an accepted-fd/exec defect from the hostfwd proxy.

## Limits

* One connection, one client; no retries in the guest (the host listener starts before QEMU).
* The fixture is x86_64 and test-only; nothing is staged in a product image.
* `tcp-accept-exec` is not present in this checkout, so the K408 acceptance cannot run it here.
