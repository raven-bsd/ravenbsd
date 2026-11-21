<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# FMA baseline: ereport bus and fault diagnosis

Status: FMA1, 2026-11-21. Evidence tags: **OBSERVED**, **PROPOSED**, **UNVERIFIED**.

This is the first bounded FMA baseline. The kernel owns a fixed-size ereport transport; diagnosis,
retire/degrade and audit policy live in userland `fmd`. Illumos FMA is read for the contract only
(CDDL), with no code copied or translated.

## 1. Ereport schema

`crates/raven-proto-fm` defines a 144-byte little-endian record:

| Offset | Field | Width |
|---:|---|---|
| 0 | magic `FM` | 2 |
| 2 | version `1` | 1 |
| 3 | severity (`minor`/`major`/`critical`) | 1 |
| 4 | class, printable ASCII, zero-padded | 32 |
| 36 | resource, printable ASCII, zero-padded | 32 |
| 68 | payload length (0..=64) | 1 |
| 69 | payload | 64 |
| 133 | timestamp milliseconds | 8 |
| 141 | reserved, must be zero | 3 |

`Ereport::encode`/`decode` are total and bounded; malformed magic, version, severity, string,
payload length or reserved byte is rejected. `EreportRing<N>` is a fixed-capacity no-allocation
queue. The kernel exposes `KernelEreportRing = EreportRing<16>` in `kernel/src/fm`; the hot path
never allocates.

## 2. Policy

`userland/servers/fmd` is the minimal userland engine. Policy is **data**, not hard-coded Illumos
behaviour:

```text
ereport.io.mmio.bad -> retire
ereport.io.ecc      -> degrade
other               -> ignore (audited)
```

Every diagnosis and every malformed rejection is written to a bounded 16-record audit trail. The
engine has no allocator and no panic path.

## 3. Injected-fault transcript

`cargo xtask qtest --case ereport --arch x86_64` builds
`build/fixtures/fmd/ereport.img` and boots the fixture as `/init`. Serial:

```text
fma: ereport class=ereport.io.mmio.bad resource=pci0:1:0 severity=major
fma: diagnosis case=1 action=retire resource=pci0:1:0
fma: diagnosis case=2 action=degrade resource=dimm0
fma: ring depth=1 restarted=yes
fma: malformed rejected
fma: restart action=retire case=1 audits=1
fma: done
init exited (0)
```

The fixture injects a bad-MMIO event (retire) and an ECC event (degrade), drains/restarts the ring,
changes the version byte to prove a malformed payload is rejected without a panic, and recreates
`fmd` to prove the policy and audit survive a restart. Serial SHA-256
`db14ba278ba91e47ea1a4ac2a85089f73929fca8e9c85f959c8d89cd55abb345`.

## 4. Commands

```sh
timeout 180 cargo nextest run -p raven-proto-fm -p raven-fmd
timeout 180 sh userland/servers/fmd/build.sh
timeout 180 cargo xtask qtest --case ereport --arch x86_64
timeout 180 cargo xtask qtest --case kernel-paging --arch x86_64
timeout 240 make check-fast
```

`driver-reload` is not present in this tree; the equivalent ring restart is exercised inside the
fixture.

## 5. Donor reconciliation

| Topic | Illumos (CDDL, behaviour only) | FreeBSD/Linux | Raven decision |
|---|---|---|---|
| ereport schema | `usr/src/uts/common/sys/fm/` event/ereport contracts | outcome only | first-party bounded 144-byte record |
| diagnosis engine | `usr/src/cmd/fm/fmd/` dispatch/case flow | outcome only | data-driven userland policy + audit |
| retire/degrade | `usr/src/cmd/fm/modules/common/*` actions | outcome only | declared action per class in fmd |
| transport | Illumos kernel event delivery | outcome only | no-alloc ring in `raven-proto-fm`, kernel alias |

No CDDL code is copied. The kernel does not diagnose, retire or read policy.

## 6. Gaps

* The kernel ring is built but not yet connected to a driver/syscall producer path; that wiring
  lives outside this ticket's Owns (`kernel/src/main.rs`/syscall/driver). The qtest proves the
  schema, policy and restart path in userland.
* `fmd` is a library plus the bounded fixture, not yet a long-running `svc:` service; service
  registration and a production `/etc/fma.policy` loader belong to the follow-up integration
  ticket.
* `driver-reload` qtest is absent in this tree; the ring restart is the available restart evidence.
* No fault was injected through real MMIO/ECC hardware; the fixture injects the two schema events
  the production producer will emit.
