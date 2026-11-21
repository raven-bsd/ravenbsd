<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# pf stateful filter KVM/TCG qtests (K296)

Status: host/QEMU evidence, 2026-11-21. **OBSERVED** on `origin/master` `6af246ac` (the K273
merge). No product behavior is changed by this ticket.

## Purpose

K273 wires the userspace pf core into netd's virtio frame path. K296 runs that same stateful
filter under KVM `-smp 2`, KVM `-smp 4` and TCG `-smp 4`, with the merged `pf` profile and the
existing one-shot probe. The cases are derived from `tests/qtest/cases/pf-core.toml`; the
accelerator and CPU count are the only differences.

## Cases

| Case | File | Accelerator | CPUs | Timeout | Result | Duration |
|---|---|---|---|---|---:|---:|
| `pf-kvm2` | `tests/qtest/cases/pf-kvm.toml` | KVM (`-enable-kvm -cpu host`) | 2 | 300 s | PASS | 24.36 s |
| `pf-kvm4` | `tests/qtest/cases/pf-kvm4.toml` | KVM (`-enable-kvm -cpu host`) | 4 | 300 s | PASS | 27.16 s |
| `pf-tcg4` | `tests/qtest/cases/pf-tcg4.toml` | TCG (no `-enable-kvm`) | 4 | 420 s | PASS | 79.92 s |

The ticket's acceptance path is `tests/qtest/cases/pf-kvm.toml`; that file's case name is
`pf-kvm2` so the lane's case-name matrix is `pf-kvm2`, `pf-kvm4`, `pf-tcg4`.

Each case asserts:

* the guest CPU count (`^smp: 2 cpus online$` or `^smp: 4 cpus online$`);
* `netd: pf loaded rules=2 nat=0`;
* the stateful allowed UDP flow through the host echo (`pf: allowed flow ok`);
* the blocked UDP flow and its serial log line
  (`netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match`);
* `pf: ok`, `svcd: settled`, `init exited (0)`;
* no `panic`/`EXCEPTION`.

## Serial evidence

```text
# build/qtest/pf-kvm2.log
22: smp: 2 cpus online
61: netd: pf loaded rules=2 nat=0
71: pf: allowed flow ok
73: netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match
74: pf: ok
78: init exited (0)

# build/qtest/pf-kvm4.log
24: smp: 4 cpus online
63: netd: pf loaded rules=2 nat=0
73: pf: allowed flow ok
75: netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match
76: pf: ok
80: init exited (0)

# build/qtest/pf-tcg4.log
24: smp: 4 cpus online
63: netd: pf loaded rules=2 nat=0
73: pf: allowed flow ok
75: netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match
76: pf: ok
80: init exited (0)
```

`build/qtest/<case>.json` records `result=pass`, `exit_status=33`, zero unmatched expectations
and zero forbidden hits. Artifacts:

| Artifact | sha256 |
|---|---|
| `build/qtest/pf-kvm2.log` | `fbc1322ec149b546a190ae5188f0a2b1b4d00876dcbefc684f1e2d263300aa07` |
| `build/qtest/pf-kvm2.json` | `379abf6f57004e567c0bad68c9da913a5dc2abb592c7912c24a0dbbbca9488d4` |
| `build/qtest/pf-kvm4.log` | `93441b66bb0fd9049a70f2df0406224730e1ad579347f689131a3271a025e75b` |
| `build/qtest/pf-kvm4.json` | `1198adb52f2cd88040cd1a3722e584f06ee5790399fdf0a145fd17735a4adf99` |
| `build/qtest/pf-tcg4.log` | `305bda96ddee35ad327df06474f5d039cb03c1d9c2fed0016bdd738b14598800` |
| `build/qtest/pf-tcg4.json` | `a78cddcd4960c91ab20c6ae4b21b311040d721fe5f5496d84cf644a474259c1d` |
| `build/qtest/pf-kvm2.img` | `6c24adf58bf114c81c861e20cbefdefa8b719f35f806d5cb9862fea723d6ff4e` |

The image hash is the same for all three cases (the profile is the same; only QEMU options
differ).

## Mutation/negative check

The 47002 `block quick` rule in `profiles/pf.d/pf.conf` was temporarily changed to `pass quick`
and `pf-kvm2` was run. The serial output changed to
`netd: pf pass out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002`; the `pf block` expectation was
unmatched and the case failed (`result=fail`, exit 1). The rule was restored, the profile diff is
clean, and `pf-kvm2` passed again (69.05 s, the image rebuild included).

## NAT gap

The merged profile has no NAT rule; all three cases correctly load `nat=0`. The contract's
"allow/block/NAT state tests" are therefore only allow/block/state under KVM/TCG; the in-guest
NAT path is not exercised by these cases. The host K273 `raven-pf`/`netd` tests cover NAT
translation (`filter_ut::nat_rewrites_the_source_and_both_checksums`,
`filter_ut::a_nat_reply_is_rewritten_back_to_the_internal_tuple`, `filter_ut::redirect_*`), but
that is not an in-guest qtest.

To add a real in-guest NAT case, two files outside K296's Owns need a change:

1. `profiles/pf.d/pf.conf`: add a NAT rule, for example
   `nat on eth0 from any to any -> 10.0.2.15` (same guest address, translated source port), so
   `netd: pf loaded rules=3 nat=1` and the stateful reply is reverse-translated.
2. `tests/fixtures/b12-netd/src/main.rs`: add a NAT assertion in the `pf` phase (or a new phase)
   that proves the translated tuple, or a serial line from netd that prints the translation.

The lead owns the profile/probe files; this ticket reports the exact paths and does not edit them.

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
timeout 420 make check-fast
```
