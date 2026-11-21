# K280 merged TCP verification

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

This directory holds the test-only cases and the artifact verifier for the merged
TCP stack verification on master `a426dee0` (K280). No product source is changed.

## Cases

The three `socket-inet-*.toml` cases are copies of `tests/qtest/cases/socket-inet.toml`
with only the accelerator and SMP count changed, so the guest TCP test and its
expectations are identical:

| Case | Accelerator | SMP | Purpose |
|---|---|---|---|
| `socket-inet-kvm-smp2.toml` | KVM | 2 | required KVM two-vCPU boot |
| `socket-inet-kvm-smp4.toml` | KVM | 4 | required KVM four-vCPU boot |
| `socket-inet-tcg-smp4.toml` | TCG | 4 | required TCG four-vCPU run |

The stock `socket-inet` qtest (TCG, `-smp 4`) is the fourth socket run. Run the
derived cases against the image built by the stock case so the image is identical:

```sh
timeout 600 cargo xtask qtest --case socket-inet
timeout 600 cargo run -q -p raven-qtest -- run \
    --case tests/tcp-merged/socket-inet-kvm-smp2.toml --image build/qtest/socket-inet.img
timeout 600 cargo run -q -p raven-qtest -- run \
    --case tests/tcp-merged/socket-inet-kvm-smp4.toml --image build/qtest/socket-inet.img
timeout 600 cargo run -q -p raven-qtest -- run \
    --case tests/tcp-merged/socket-inet-tcg-smp4.toml --image build/qtest/socket-inet.img
```

## Artifact verifier

`verify-results.py` checks that every required qtest JSON is present and terminal
and that every K253 scenario has a non-empty pcap and guest serial log. A test
`fail` is preserved as a finding; a missing or malformed artifact is a verifier
failure.

```sh
timeout 60 python3 tests/tcp-merged/verify-results.py --selftest
timeout 60 python3 tests/tcp-merged/verify-results.py \
    --qtest-dir build/qtest \
    --netconf-dir /tmp/opencode/k280-netconf \
    --netconf-log-dir /tmp/opencode
```

The selftest accepts a complete fixture and rejects both a missing qtest JSON and
an empty pcap, which is the negative check for this verifier.

## Scope

* K253 five-scenario matrix: `handshake`, `zero-window`, `window-update`,
  `retransmit`, `fin-rst` through `scripts/net-conformance.sh` (read-only).
* Qtests: `socket-inet`, `net-host` (tap selftest), `fetch-https`.
* Gates: `make check-fast`, nextest results are in the K280 report.

No donor code is copied; this is verification test code only.
