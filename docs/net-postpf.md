<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Post-K273 network regression (K371)

Status: **BLOCKED** for the default release-image tap proof; **PASS** for the merged pf
profile's staged allowed/denied flow. This is a tests/evidence record, not a product fix.

K273 wired `raven-pf` into netd's ingress/egress path. K371 checks whether the default release
image (no `/etc/pf.conf`) still brings up netd/DHCP and host/guest ping, and whether the pf profile
allows one flow and denies another.

## Results

| Check | Result | Evidence |
|---|---|---|
| tap/bridge prerequisites | PASS | `qemu-net: check PASS`; `tests/evidence/network/postpf/qemu-net-check.log` |
| release image build | **BLOCKED / TIMEOUT** | 300 s and 600 s caps both expired while compiling uutils/ICU; `image-build-300.log`, `image-build-600.log` |
| default release tap boot + host/guest ping | **NOT RUN** | no current release image; no stale image used |
| default netd without a file service | **FAIL-CLOSED finding** | `netd: pf config: file service is not ready; refusing to start` on `netd-virtio`; `netd-virtio-no-fileservice.log` |
| pf profile allowed/denied flow | **PASS** (retry) | `pf-core: pass`; serial has `pf loaded rules=2`, `pf: allowed flow ok`, `pf block ... 47002`, `pf: ok`, `init exited (0)` |

## Exact blocked lines

Release image build, both caps:

```text
timeout 300 cargo xtask image --arch x86_64 --profile release --output build/qtest/net-postpf.img
# exit 124, still in libc/uutils target build
timeout 600 cargo xtask image --arch x86_64 --profile release --output build/qtest/net-postpf.img
# exit 124, still compiling ICU/uutils
```

Default netd on a profile with no file service (`netd-virtio`):

```text
netd: pf config: file service is not ready; refusing to start
netd: pf config: file service is not ready; refusing to start
init exited (1)
```

The release profile inherits `shell.d/30-initfsd`; the small `netd-virtio` profile does not.
So this is a **test-profile/profile-hook regression**, not proven to be the release image. A
release boot is required to distinguish `netd: pf absent` from the fail-closed path.

## PF profile evidence

`cargo xtask qtest --case pf-core --arch x86_64` passed on the second run (the first run hit the
host UDP-echo deadline before the guest probe and is preserved as a flake):

```text
netd: pf loaded rules=2 nat=0
pf: allowed flow ok
pf: blocked datagram sent
netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match
pf: ok
init exited (0)
```

The staged ruleset is UDP 47001/47002. The ticket's ICMP wording does not match the profile, and
K371 may not edit product profiles, so the ICMP allow/deny variant is an open input rather than a
claimed pass.

## Next actions

1. Build `--profile release` on an idle host with a warm uutils/ICU target cache (or extend the
   build budget via a measured, recorded cap), then run the K194 tap selftest on that image:
   `RAVEN_QEMU_IMAGE=build/qtest/net-postpf.img sh tests/scripts/qemu-net-selftest.sh`.
2. Decide whether the netd pf-config loader should treat `NotReady` after the deadline as "no
   ruleset" when the profile has no file service, or whether every netd profile must stage
   initfsd. The current behavior is fail-closed; the release profile includes initfsd.
3. If the ICMP allow/deny proof is required, it needs a pf ruleset with ICMP rules; that is a
   product/profile edit outside K371's Owns.

## Evidence

Preserved under `tests/evidence/network/postpf/` with `SHA256SUMS`; no product source was edited.
