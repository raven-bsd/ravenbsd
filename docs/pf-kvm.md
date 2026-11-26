<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# pf stateful filter KVM/TCG qtests (K296)

Status: host/QEMU evidence, 2026-11-21. **OBSERVED** on `origin/master` `6af246ac` (the K273
merge) plus the K296 rework. No product behavior is changed by this ticket.

## Purpose

K273 wires the userspace pf core into netd's virtio frame path. K296 runs that same stateful
filter under KVM `-smp 2`, KVM `-smp 4` and TCG `-smp 4`, with the merged `pf` profile and the
existing one-shot probe. The cases are derived from `tests/qtest/cases/pf-core.toml`; the
accelerator and CPU count are the only differences. The rework adds an in-guest NAT flow.

## Cases

| Case | File | Accelerator | CPUs | Timeout | Result | Duration |
|---|---|---|---|---|---:|---:|
| `pf-kvm2` | `tests/qtest/cases/pf-kvm.toml` | KVM (`-enable-kvm -cpu host`) | 2 | 300 s | PASS | 17.06 s |
| `pf-kvm4` | `tests/qtest/cases/pf-kvm4.toml` | KVM (`-enable-kvm -cpu host`) | 4 | 300 s | PASS | 11.72 s |
| `pf-tcg4` | `tests/qtest/cases/pf-tcg4.toml` | TCG (no `-enable-kvm`) | 4 | 420 s | PASS | 80.08 s |

The ticket's acceptance path is `tests/qtest/cases/pf-kvm.toml`; that file's case name is
`pf-kvm2` so the lane's case-name matrix is `pf-kvm2`, `pf-kvm4`, `pf-tcg4`.

Each case asserts:

* the guest CPU count (`^smp: 2 cpus online$` or `^smp: 4 cpus online$`);
* `netd: pf loaded rules=3 nat=1`;
* the stateful allowed UDP flow through the host echo (`pf: allowed flow ok`);
* the NAT flow (`pf: nat flow ok port=NNNNN`) through the `nat` rule and the host NAT echo;
* the blocked UDP flow and its serial log line
  (`netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match`);
* `pf: ok`, `svcd: settled`, `init exited (0)`;
* no `panic`/`EXCEPTION`.

## Serial evidence

```text
# build/qtest/pf-kvm2.log
22: smp: 2 cpus online
61: netd: pf loaded rules=3 nat=1
71: pf: allowed flow ok
73: pf: nat flow ok port=59816
75: netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match
76: pf: ok
80: init exited (0)

# build/qtest/pf-kvm4.log
24: smp: 4 cpus online
63: netd: pf loaded rules=3 nat=1
73: pf: allowed flow ok
75: pf: nat flow ok port=54028
77: netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match
78: pf: ok
82: init exited (0)

# build/qtest/pf-tcg4.log
24: smp: 4 cpus online
63: netd: pf loaded rules=3 nat=1
73: pf: allowed flow ok
75: pf: nat flow ok port=44118
77: netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match
78: pf: ok
82: init exited (0)
```

`build/qtest/<case>.json` records `result=pass`, `exit_status=33`, zero unmatched expectations
and zero forbidden hits. Artifacts:

| Artifact | sha256 |
|---|---|
| `build/qtest/pf-kvm2.log` | `37ab65c6db4fac65564ed27286a95acd6864383768a43a61bdf4ece8f3b2d050` |
| `build/qtest/pf-kvm2.json` | `f0dfe9b72da61b0a1f19f9e36c393ae351ab3260979432094f6201b4e24abd5c` |
| `build/qtest/pf-kvm4.log` | `98b2c1f4f9e4156a5ac7836d04774dcbec9a64e883c3ade025fca09b0c6891f3` |
| `build/qtest/pf-kvm4.json` | `1c118b11c00e2a9091dfb20852a102a9d21b265b13d4577d76a737a5f6a94af5` |
| `build/qtest/pf-tcg4.log` | `951ecf15c2039f238b19f4a72d4ce8ada2327e31d48f43c5bb27b60dfa6d90ab` |
| `build/qtest/pf-tcg4.json` | `0a8d03bec070f0248c8286ec915715fd5cfc8668aff761100adbc15e5022ad72` |
| `build/qtest/pf-core.log` | `0af45d7f4bea2af07f12376cb67cb837d0a5e9f21038b297a56147bf4e2d7b7c` |
| `build/qtest/pf-core.json` | `9e91db144a6b31aec23b36412421921d1319cca914bb288cf67e47693d8d181a` |

## NAT flow

The merged `pf` profile now carries:

```text
pass out on eth0 proto udp from any to any port 47004 keep state
nat on eth0 proto udp from any to 10.0.2.2 port 47004 -> 10.0.2.15
```

The `tests/fixtures/b12-netd/src/main.rs` `pf` phase adds `check_pf_nat`: it binds
`10.0.2.15:7204`, sends a UDP datagram to `10.0.2.2:47004`, and reads the reply. The host NAT
echo (`tests/qtest/cases/pf-kvm-fixture/nat_echo.py`) replies with the source port it observed.
The probe asserts the reply comes from the original `47004` endpoint and that the observed source
port differs from its local `7204`, then prints `pf: nat flow ok port=NNNNN`.

The host echo log (`/tmp/k296-nat-echo.log`) records the datagram, for example:

```text
nat-echo ('127.0.0.1', 36108)
```

**Evidence limitation:** QEMU user-mode networking (SLIRP) also rewrites the outbound source port,
so the host-observed port is not by itself a pf-specific translation proof. The case's primary
NAT assertions are the `nat=1` load line and the matching `nat` rule with the stateful flow; the
port value is a sanity check. A translation-specific test independent of SLIRP would need an
inbound `rdr` rule plus a host-forwarded sender (next step, not part of this rework).

## Mutation/negative check

The `nat` rule was temporarily commented out and `pf-kvm2` was run. `netd` then reported
`netd: pf loaded rules=3 nat=0`; the `nat=1` expectation was unmatched and the case failed
(`result=fail`, exit 1). The rule was restored, the profile diff is clean, and `pf-kvm2` passed
again (17.06 s, `pf: nat flow ok port=59816`).

## pf-core compatibility

The shared `pf` profile now loads `rules=3 nat=1`, so the existing `tests/qtest/cases/pf-core.toml`
load-line expectation was updated from `rules=2 nat=0` to `rules=3 nat=1`, and its `pre` starts
the same one-shot NAT echo. `pf-core` still passes (`pf-core: pass`, 87.05 s, `pf: nat flow ok
port=36108`). This is a shared-profile consequence, not a new product behavior.

## Ticket command spelling

The ticket's acceptance command is written as
`cargo xtask qtest run --case tests/qtest/cases/pf-kvm.toml`. The xtask wrapper does not accept a
leading `run` (the harness subcommand is already added by xtask); the command exits 2 with
`error: qtest: unexpected argument 'run'`. The supported equivalent used here is:

```sh
timeout 600 cargo xtask qtest --case tests/qtest/cases/pf-kvm.toml --arch x86_64
```

## Donor reconciliation

Test-only. The pf state/NAT semantics are K273's port of FreeBSD
`133344405f532bd54ff3629a77d4c4e062f01fdb`, `sys/netpfil/pf/pf.c` (`pf_test`,
`pf_test_rule`, `pf_create_state`, `pf_find_state`) and `sys/netpfil/pf/pf_lb.c`
(`pf_get_transaddr`), BSD-2-Clause. Illumos is CDDL behaviour only; Linux is GPL outcome only.
No donor code is copied. The QEMU harness and these cases are first-party test code.

## Repro

```sh
timeout 600 cargo xtask qtest --case tests/qtest/cases/pf-kvm.toml --arch x86_64
timeout 600 cargo xtask qtest --case tests/qtest/cases/pf-kvm4.toml --arch x86_64
timeout 600 cargo xtask qtest --case tests/qtest/cases/pf-tcg4.toml --arch x86_64
timeout 600 cargo xtask qtest --case tests/qtest/cases/pf-core.toml --arch x86_64
timeout 420 make check-fast
```
