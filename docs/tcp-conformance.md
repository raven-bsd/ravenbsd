<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Packet-level TCP conformance harness (K253/K275)

`tests/net-conformance/harness.py` drives the tested TCP connection as raw
Ethernet/IPv4/TCP frames from an `AF_PACKET` socket. The peer is not a host
kernel socket, so every segment, option, advertised window and ACK is chosen by
the test. The same scenarios run against:

* the host kernel as the reference baseline, in a disposable network namespace
  (a veth pair, one end in a namespace that only runs the baseline server); and
* the Raven guest over the K227 tap (`10.0.2.15`, gateway `10.0.2.2`).

The host-side script re-executes in a user+net+mount namespace, creates the tap,
and starts QEMU. This keeps the tap privilege out of the test code and isolates
the baseline from the host network.

## Scenarios

| Scenario | Wire assertion |
|---|---|
| `handshake` | SYN-ACK has MSS and SACK; a peer that offered window scaling gets a scale, a peer that did not offer it gets **no** window-scale option and an unscaled window |
| `zero-window` | after the final ACK advertises zero, the server makes a persist/window probe; the probe byte is accepted and the window reopened; the reassembled stream is exactly the 44-byte fixture, and the accepted probe byte is not replayed |
| `window-update` | a stale zero window followed by a nonzero update must flush all 44 bytes |
| `retransmit` | the first 44-byte segment is left unacknowledged; an RTO retransmission carries the same sequence and bytes |
| `fin-rst` | the server delivers 44 bytes, accepts a peer FIN and returns its own FIN |

`dup-ack`, `out-of-order`, `partial-ack`, `backlog` and `busy` are narrowed, not
faked. The reasons are printed by `--scenario <name>` and are listed in
`NARROWED` in the harness: a single raw client cannot make the receiver see
duplicate ACKs, the fixture has one in-order server write, a partial ACK needs a
multi-segment write with selective loss, the guest listener accepts one
connection at a time, and device-busy injection needs a driver shim.

## Running one scenario

```sh
timeout 120 sh scripts/net-conformance-selftest.sh
timeout 300 sh scripts/net-conformance.sh --scenario handshake --baseline linux
timeout 300 sh scripts/net-conformance.sh --scenario zero-window --baseline linux
timeout 300 sh scripts/net-conformance.sh --scenario zero-window --target raven
```

Add `--no-build` to reuse `build/qtest/net-conformance.img`; the target build
uses the existing `network-config-test` profile and overlays only
`/bin/netconf` and `/etc/services.d/netconf.toml` from
`tests/fixtures/net-conformance/`. No profile or product source is changed.

## Running a selected branch, worktree or image (K275)

`scripts/net-conformance.sh` accepts a selected target instead of the current
checkout:

```sh
# Build a detached worktree for the ref, then run one scenario against it.
timeout 420 sh scripts/net-conformance.sh \
    --branch origin/kevin/K271 --scenario zero-window --target raven

# Reuse an existing worktree (its fixture is overlaid unless
# NETCONF_USE_WORKTREE_FIXTURE=1 is set).
timeout 420 sh scripts/net-conformance.sh \
    --worktree /path/to/worktree --scenario zero-window --target raven

# Run a ready image, without building anything.
timeout 300 sh scripts/net-conformance.sh \
    --image /path/to/net-conformance.img --scenario zero-window --target raven \
    --no-build

# Build only and print the image path (used by the matrix runner).
timeout 600 sh scripts/net-conformance.sh \
    --branch origin/kevin/K271 --build-only
```

A `--branch` build uses a detached worktree under
`${NETCONF_WORKTREE_ROOT:-/tmp/opencode/k275-worktrees}` and the current
checkout's `target/` (unless `CARGO_TARGET_DIR` is set), so port branches reuse
the compiled toolchain. The host harness and the test-only guest fixture come
from the current checkout; the selected branch supplies only product code. This
is why the runner can compare lanes without editing `crates/raven-tcpip`.

## Full matrix and evidence

`tests/net-conformance/harness/run.py` runs all five scenarios and writes one
pcap, one guest serial log and one command log per scenario, plus
`summary.json` and `summary.md`:

```sh
timeout 420 python3 tests/net-conformance/harness/run.py \
    --branch origin/master --label master
timeout 360 python3 tests/net-conformance/harness/run.py \
    --image /path/to/image --label kevin-K271
timeout 360 python3 tests/net-conformance/harness/run.py \
    --worktree /path/to/worktree --label lane
```

Results live under `tests/net-conformance/results/k275/<label>/` and are
git-ignored. The summary records the commit, image sha256, per-scenario result,
the failing detail and the pcap/serial paths. A non-zero runner exit means at
least one scenario failed; that is normal for current master and is the evidence
sent to the owning lane. Each inner scenario is itself capped by
`--scenario-timeout` (default 180 s); the build is capped by `--build-timeout`
(default 900 s).

## Recorded results

`origin/master` `4adc24a2` and `origin/kevin/K271` `55184dc6` produce the same
five-scenario result on the ported stack:

| Scenario | Result | Detail |
|---|---|---|
| handshake | FAIL | `SYN-ACK mss=1460 ws=1 sack=True; plain-peer ws=1 win=65535` |
| zero-window | FAIL | `probe=1B seq=1; after-reopen=43B; total=44/44; replayed-probe=yes` |
| window-update | PASS | `delivered=44/44 frames=1` |
| retransmit | PASS | `first=44B seq+0; retransmit=yes` |
| fin-rst | PASS | `delivered=44/44; peer-close=FIN` |

The handshake failure belongs to the window-scale negotiation path; the
zero-window failure is the accepted probe byte replayed from `snd_una`. Both are
reported to the owning TCP lane; this ticket changes no product code.

## Scenario references

The harness is original test code. The scenario expectations were read from
behaviour, not copied:

| Topic | Illumos (CDDL, behaviour only) | BSD (permissive reference) | Linux (GPL, read-only outcome) |
|---|---|---|---|
| SYN/SYN-ACK options, window scale | `usr/src/uts/common/inet/tcp/tcp_output.c` `tcp_output`, `tcp_input.c` `tcp_input` option negotiation | FreeBSD `sys/netinet/tcp_output.c` `tcp_output`, `tcp_input.c` `tcp_dooptions`; NetBSD equivalents | `net/ipv4/tcp_output.c` `tcp_connect`/`tcp_transmit_skb`, `tcp_input.c` `tcp_parse_options` |
| Zero-window persist/reopen | `tcp_output.c` persist path, `tcp_timer.c` `tcp_timer` | FreeBSD `tcp_output.c` `tcp_output`/`tcp_setpersist`, `tcp_timer.c` `tcp_timer_persist`, `tcp_input.c` window update | `tcp_output.c` `tcp_write_xmit`/`tcp_send_probe0`, `tcp_timer.c` `tcp_write_timer`, `tcp_input.c` `tcp_ack` |
| RTO retransmission | `tcp_output.c`/`tcp_input.c` behaviour | FreeBSD `tcp_output.c` `tcp_output`, `tcp_timer.c` `tcp_timer_rexmt`, `tcp_input.c` `tcp_ack` | `tcp_output.c` `tcp_retransmit_skb`, `tcp_input.c` `tcp_ack` |
| FIN/half-close | `tcp_output.c`/`tcp_input.c` behaviour | FreeBSD `tcp_output.c` `tcp_output`, `tcp_subr.c` `tcp_close`, `tcp_input.c` FIN path | `tcp_output.c` `tcp_send_fin`, `tcp_input.c` reset path |

No donor code is linked, vendored or translated. The Linux baseline is a black
box running the host kernel.
