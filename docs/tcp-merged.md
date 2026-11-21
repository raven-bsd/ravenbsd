<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Merged TCP stack verification (K280)

Verification of the first merged TCP port wave on `origin/master`
`a426dee0`. Branch `mel/K280` adds only test cases and this document; no product
source is changed.

## Environment

| Item | Value |
|---|---|
| Master base | `a426dee0` |
| K280 branch commit | `e8a7732f` (test cases) + report commit |
| Kernel | Linux `5.19.0-50-generic` |
| QEMU | `6.2.0` (Debian `1:6.2+dfsg-2ubuntu6.31`) |
| KVM | `/dev/kvm` present and writable; `-accel kvm` boots |
| OVMF | `/usr/share/OVMF/OVMF_CODE_4M.fd` and `OVMF_VARS_4M.fd` |
| Release image | built from `a426dee0`, sha256 prefix `702cebf52e29b169` |

The host was shared with other active TCP lanes; load average ranged from ~36 to
~112 during the runs. The qtest durations below are therefore upper bounds.

## Gate and socket tests

| # | Command | Exit | Result | Evidence |
|---|---|---|---|---|
| 1 | `timeout 420 make check-fast` | 0 | `check-fast: PASSED` | `/tmp/opencode/k280-checkfast.log` |
| 2 | `timeout 600 cargo xtask qtest --case socket-inet` | 1 | qtest `fail`, guest `socket errno=38`, exit status 35 | `build/qtest/socket-inet.json` |
| 3 | `timeout 600 cargo run -q -p raven-qtest -- run --case tests/tcp-merged/socket-inet-kvm-smp2.toml --image build/qtest/socket-inet.img` | 1 | KVM `-smp 2`, same `errno=38`, exit status 35 | `build/qtest/socket-inet-kvm-smp2.json` |
| 4 | `timeout 600 ... socket-inet-kvm-smp4.toml ...` | 1 | KVM `-smp 4`, same `errno=38`, exit status 35 | `build/qtest/socket-inet-kvm-smp4.json` |
| 5 | `timeout 600 ... socket-inet-tcg-smp4.toml ...` | 1 | TCG `-smp 4`, same `errno=38`, exit status 35 | `build/qtest/socket-inet-tcg-smp4.json` |

The ticket's `cargo xtask qtest run --case ...` spelling is rejected by the
xtask front-end (`qtest: unexpected argument 'run'`); the supported spelling is
`cargo xtask qtest --case ...`. The derived KVM/TCG cases use the underlying
`raven-qtest run` form so `-accel`/`-smp` can be set in the case.

## Tap and HTTPS qtests

| # | Command | Exit | Result | Evidence |
|---|---|---|---|---|
| 6 | `timeout 600 cargo xtask qtest --case net-host` | 1 | pre `qemu-net-selftest.sh` fails: host->guest ping passes, guest->host `ping -c 3 10.0.2.2` has no 0% loss | `build/qemu-net/serial.log`, `build/qtest/net-host.json` |
| 7 | `timeout 600 cargo xtask qtest --case fetch-https --no-build` | 1 | `curl: connection failed`, `curl: TLS: TLS handshake failed`, `curl-head=1` | `build/qtest/fetch-https.log`, `.json` |

`net-host` first failed against the stale `build/x86_64/ravenbsd-x86_64.img`
from 2026-11-19 (no getty); the release image was rebuilt from `a426dee0` before
the result above. The retry reached the tap selftest and failed only on
guest->host reachability.

One earlier `fetch-https` run reached `HTTP/1.1 200` and `wget-https=0` before the
600 s outer cap; two clean reruns (`--no-build`) fail at the first TLS connect.
The run is recorded as a network/TLS finding, not a pass.

## K253 five-scenario matrix

Built once from the current tree, then reused with `--no-build`:

```sh
NETCONF_TAP=nctap1 timeout 600 sh scripts/net-conformance.sh \
    --scenario handshake --target raven
NETCONF_TAP=nctap1 timeout 300 sh scripts/net-conformance.sh \
    --scenario <name> --target raven --no-build \
    --pcap /tmp/opencode/k280-netconf/<name>.pcap \
    --serial /tmp/opencode/k280-netconf/<name>.serial.log
```

| Scenario | Exit | Wire result | Artifact |
|---|---:|---|---|
| `handshake` | 1 | FAIL: `SYN-ACK mss=1460 ws=1 sack=True; plain-peer ws=1 win=65535` | `handshake.pcap`, `handshake.serial.log` |
| `zero-window` | 0 | PASS: `probe=1B seq=1; after-reopen=43B; total=44/44; frames=1; replayed-probe=no` | `zero-window.pcap` |
| `window-update` | 0 | PASS: `delivered=44/44 frames=1` | `window-update.pcap` |
| `retransmit` | 0 | PASS: `first=44B seq+0; retransmit=yes` | `retransmit.pcap` |
| `fin-rst` | 0 | PASS: `delivered=44/44; peer-close=FIN` | `fin-rst.pcap` |

The zero-window replay from the pre-merge K253 report is fixed. The handshake
finding remains: a peer that did not offer window scaling still receives
`wscale=1`.

## Findings for the owning lanes

| Finding | Evidence | Suggested owner |
|---|---|---|
| AF_INET `socket()` returns `ENOSYS` (`errno=38`) in the merged `network-config-test` image; all four socket runs are identical | qtest JSONs 2-5, guest line `socket-inet: socket errno=38` | K230 / ravenlibc socket path or the merged syscall lane |
| Guest cannot reach the host gateway `10.0.2.2`: host->guest ICMP works, guest->host ping fails | `build/qemu-net/serial.log` | K227/K262 netd/network |
| HTTPS to `10.0.2.2:24443` fails at TCP connect/TLS handshake; one run reached HTTP 200, so the path is flaky | `build/qtest/fetch-https.log` | K262/K265 network stack |
| Plain (no-WS) peer is offered `wscale=1` | `handshake.pcap` | TCP option negotiation (K253/K265) |

## Verifier and negative check

`tests/tcp-merged/verify-results.py` validates artifact presence and terminal
qtest results; it does not convert a failing test into a pass.

```text
$ timeout 60 python3 tests/tcp-merged/verify-results.py --selftest
verify-results: selftest PASS (valid fixture accepted; missing/empty rejected)
exit 0

$ timeout 60 python3 tests/tcp-merged/verify-results.py \
    --qtest-dir build/qtest \
    --netconf-dir /tmp/opencode/k280-netconf \
    --netconf-log-dir /tmp/opencode
verify-results: 0 artifact failure(s)
exit 0
```

The selftest is the negative check: it removes a required qtest JSON and empties
a pcap, and both mutations make the verifier return non-zero before the valid
fixture is accepted.

## Donor reconciliation

This is verification, not a port. No code is copied or translated.

| Topic | Illumos (behaviour only) | FreeBSD/NetBSD (permissive reference) | Linux (read-only outcome) | Raven decision |
|---|---|---|---|---|
| SYN/SYN-ACK options, window scale | `usr/src/uts/common/inet/tcp/tcp_output.c` `tcp_output`, `tcp_input.c` option negotiation | NetBSD `sys/netinet/tcp_output.c` `tcp_output`, `tcp_input.c` `tcp_dooptions` | `net/ipv4/tcp_output.c`, `tcp_input.c` (`tcp_parse_options`) | raw-frame assertion; no code copied |
| Zero-window persist/reopen | `tcp_output.c` persist path, `tcp_timer.c` | NetBSD `tcp_output.c` `tcp_setpersist`, `tcp_timer.c`, `tcp_input.c` | `tcp_output.c` `tcp_send_probe0`, `tcp_input.c` `tcp_ack` | exact 44-byte stream, probe accepted once |
| RTO retransmission | `tcp_output.c`/`tcp_input.c` behaviour | NetBSD `tcp_output.c`/`tcp_input.c` | `tcp_output.c` `tcp_retransmit_skb` | same seq/bytes |
| FIN/half-close | `tcp_output.c`/`tcp_input.c` | NetBSD `tcp_output.c`/`tcp_subr.c`/`tcp_input.c` | `tcp_output.c` `tcp_send_fin` | peer FIN answered with FIN |

Donor revisions: FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`, Illumos
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`. No donor file was copied, translated
or vendored.
