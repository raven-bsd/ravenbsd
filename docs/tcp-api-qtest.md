<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP API qtest matrix (K294)

Focused TCP API/boot matrix for the release gate on `origin/master`
`ba38fb70`. The cases are test-only and live under `tests/qtest/cases/tcp-api*`;
no product source is changed.

## Cases

| Case | Fixture | Path exercised | TCG | KVM |
|---|---|---|---|---|
| `tcp-api.toml` | `socket-inet` | `socket`, `bind`, `listen`, `poll`, `accept`, `read`, `write`, `close` | yes | n/a |
| `tcp-api-kvm.toml` | `socket-inet` | same as `tcp-api.toml` | n/a | yes |
| `tcp-api-half-close.toml` | `netconf` | peer FIN after the payload, guest `read=0`, `close` | yes | n/a |
| `tcp-api-half-close-kvm.toml` | `netconf` | same as `tcp-api-half-close.toml` | n/a | yes |
| `tcp-api-backlog.toml` | `netconf` | four queued connections, four accepted/closed pairs | yes | n/a |
| `tcp-api-backlog-kvm.toml` | `netconf` | same as `tcp-api-backlog.toml` | n/a | yes |

The `netconf` cases use the existing K253 guest listener on
`10.0.2.15:23090` and map it to host `127.0.0.1:23090` with QEMU user
networking. The host peer scripts are
`tests/qtest/cases/tcp-api-half-close-peer.sh` and
`tests/qtest/cases/tcp-api-backlog-peer.sh`.

The `netconf` service is a long-lived listener, so those cases use
`expect_exit = "timeout"` and assert the serial markers before the cap. Durations
are therefore the cap (60 s TCG, 45 s KVM), not the boot time.

The case `pre` steps build the fixture only when its initfs is absent
(`test -f ... || sh .../build.sh`), so repeated runs do not contend on the cargo
lock while another lane is building.

## Commands and results

```text
$ timeout 400 cargo xtask qtest --case tcp-api --no-build
tcp-api: pass
exit 0, 35436 ms

$ timeout 400 cargo run -q -p raven-qtest -- run \
    --case tests/qtest/cases/tcp-api-kvm.toml --image build/qtest/tcp-api.img
tcp-api-kvm: pass
exit 0, 17637 ms

$ timeout 120 cargo run -q -p raven-qtest -- run \
    --case tests/qtest/cases/tcp-api-half-close.toml \
    --image build/qtest/net-conformance.img
tcp-api-half-close: pass
exit 0, 60038 ms

$ timeout 90 cargo run -q -p raven-qtest -- run \
    --case tests/qtest/cases/tcp-api-half-close-kvm.toml \
    --image build/qtest/net-conformance.img
tcp-api-half-close-kvm: pass
exit 0, 45117 ms

$ timeout 120 cargo run -q -p raven-qtest -- run \
    --case tests/qtest/cases/tcp-api-backlog.toml \
    --image build/qtest/net-conformance.img
tcp-api-backlog: pass
exit 0, 60029 ms

$ timeout 90 cargo run -q -p raven-qtest -- run \
    --case tests/qtest/cases/tcp-api-backlog-kvm.toml \
    --image build/qtest/net-conformance.img
tcp-api-backlog-kvm: pass
exit 0, 45112 ms
```

Artifacts: `build/qtest/tcp-api*.json` and `build/qtest/tcp-api*.log`;
host-side peer logs under `/tmp/opencode/tcp-api-*-peer.log`.

## Negative check

A temporary copy of `tcp-api-half-close.toml` changed the expected guest line
from `netconf: peer-event=... read=0` to `read=9`. The qtest failed with the
unmatched expectation, proving the serial assertion catches a wrong half-close
observation; the repository case was not modified and still passes. The exact
command and output are in the K294 report.

## Donor reconciliation

Test-only. No donor code is copied, translated or vendored.

| Topic | Illumos (behaviour only) | FreeBSD/NetBSD (permissive reference) | Linux (read-only outcome) | Raven decision |
|---|---|---|---|---|
| TCP connect/accept/send/recv/close | `usr/src/uts/common/inet/tcp/tcp.c` socket lifecycle, `tcp_output.c`/`tcp_input.c` | NetBSD `sys/netinet/tcp_usrreq.c`, `tcp_input.c`, `tcp_output.c` | `net/ipv4/tcp.c`, `tcp_input.c`, `tcp_output.c` | qtest over the merged K266/K271 socket path; no code copied |
| listener backlog | TCP listen/accept queue behaviour | NetBSD `tcp_usrreq.c` `tcp_listen`, `tcp_input.c` SYN queue | `net/ipv4/tcp.c` `inet_listen`, `tcp_minisocks.c` | four concurrent connections before accept; wire/console assertion |
| half-close | FIN state transitions | NetBSD `tcp_input.c` FIN path, `tcp_subr.c` close | `tcp_input.c` `tcp_fin`, `tcp.c` shutdown | peer FIN -> guest `read=0` and close; no code copied |
| boot/accelerator matrix | n/a | n/a | n/a | qtest TCG/KVM variants; no donor code |

No `PROVENANCE.toml` entry is needed: these are original test cases and host peer
scripts. Donor revisions referenced by the port lanes: FreeBSD
`133344405f532bd54ff3629a77d4c4e062f01fdb`, NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`, Illumos
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`.
