<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Service grants: the PCI root and the Pi 4 platform root (P10)

`svcd` (PID 1, `userland/init`) grants capabilities to a service only from the service's
profile-managed manifest, before the service's first thread starts. The policy is pure and lives
in `userland/init/src/grants.rs`; the host tests are in `grants_ut.rs`.

P10 splits the old single root grant into two named capabilities. The motivation is P8's
`devd dtb launch`: the Pi 4 platform launcher must create MMIO, DMA and IRQ handles, but the old
policy reserved the device root for the service name `pcid`, so the qtest had to run `devd` under
the `pcid` name. Name spoofing is not an acceptable production policy.

## Capability matrix

| Manifest grant | Service allowed | Handle | Rights | Use |
|---|---|---|---|---|
| `initfs` | any service | `INITFS` | `READ\|MAP\|EXEC\|TRANSFER` | map the boot archive |
| `root` | `pcid` only | `ROOT_DEVICE` | `ADMIN\|DUP\|TRANSFER` | PCI enumeration and driver launch |
| `platform-root` | `devd` only | `ROOT_DEVICE` | `ADMIN` | Pi 4 device-tree platform launch |

Any other combination is rejected before the service is spawned:

| Request | Result |
|---|---|
| `logd` declares `platform-root` | error: `platform-root grant reserved for devd` |
| `pcid-helper` declares `root` | error: `root grant reserved for pcid` |
| `pcid` declares `platform-root` | error: `platform-root grant reserved for devd` |
| `devd` declares `root` | error: `root grant reserved for pcid` |
| `devd` declares no device grant | no device handle (the release and server entries) |
| unknown name | error: `unknown grant <name>` |

`devd`'s platform capability is deliberately narrower than `pcid`'s PCI capability. The kernel
checks `ADMIN` for `mmio_create`, `dma_alloc`, `irq_create` and `boot_blob`; `devd` needs exactly
that. It never duplicates or transfers the root capability itself. Each matched driver receives
only the individual `MmioRegion`, `DmaBuffer` and `Irq` handles that `devd` created for it, so the
root capability is not ambient in the driver processes.

## How a profile allows a grant

A service manifest is part of the profile-built initfs and is trusted build input, not something
a running process can write or change. A profile allows a device grant by staging the one
manifest entitled to declare it:

* the QEMU release and server profiles stage `profiles/server.d/devd.manifest` and the shared
  `etc/services.d/devd.toml`, neither of which declares `platform-root`;
* the Pi 4 platform profile (REL3's file, not P10) must stage its own `devd` entry with
  `args = ["devd", "dtb", "launch"]` and `grants = ["platform-root"]`;
* the P10 qtest stages `tests/fixtures/pi4-devd/etc/devd.toml` with the real `name = "devd"` and
  `grants = ["platform-root"]`.

The policy additionally pins each capability to one service name, so a manifest for another
service cannot obtain it by declaring it. `svcd` rejects a duplicate service name before the
start plan, so a profile cannot add a second `devd` entry beside the real one.

The grant must be the only grant on the `devd` entry. A fresh child has handles 0 (self),
1 (initfs) and 2 (namespace); `platform-root` then lands at handle 3, which is
`Handle::ROOT_DEVICE` and the slot `devd`'s platform launch uses. Adding `initfs` to the same
entry would shift the root handle, so the policy rejects any `platform-root` entry that also
declares another grant.

## Threat model

Trusted:

* PID 1 and the pure grant policy compiled into it;
* the initfs archive and the service manifests it carries;
* the profile that decides which manifests are staged;
* the kernel's root-device checks (`ADMIN` and the RAM/kernel-memory refusal).

Untrusted or less trusted:

* a service process after spawn: it cannot request more grants, rename itself or make PID 1
  transfer another capability;
* a driver spawned by `devd`: it receives only the handles for its selected device window, DMA
  buffer and interrupt, and never the root device handle;
* input to `devd` from the device tree and `/etc/drivers.toml`: parsed and bounded before any
  handle is created.

Properties:

1. **No name spoofing.** The grant decision uses the manifest's service name. `root` is still
   `pcid`-only; `platform-root` is `devd`-only. A different service cannot ask for either, and a
   process cannot change the manifest that named it.
2. **Least privilege for the new path.** `platform-root` is `ADMIN` only, not `pcid`'s
   `ADMIN|DUP|TRANSFER`, so `devd` cannot hand the root capability to a driver or another
   process. It can only create child capabilities.
3. **No ambient inheritance.** PID 1 transfers grants explicitly at child creation. A fresh child
   starts with handles 0, 1 and 2 only; drivers get the handles `devd` created, not handle 3.
4. **Profile opt-in.** The release and server profiles do not declare the capability; only a
   platform profile that stages the Pi 4 `dtb launch` entry does.

Residual risks:

* The kernel root device capability is not range-scoped. A compromised `devd` could create MMIO,
  DMA or IRQ handles outside the windows decoded from the device tree. The policy limits *who*
  holds the capability, not *what* it can address. A future range-limited platform capability
  would close that gap; it is outside P10 and would change `crates/raven-abi`/the kernel.
* `devd` is first-party code and its input is the firmware device tree plus a root-owned
  configuration file, but a malicious DTB is still parsed in the privileged process. The parser
  rejects unmapped ranges and unknown selectors before creating any handle.
* The production Pi 4 profile entry is REL3's file. P10 supplies the policy and the qtest proof;
  REL3 must declare `grants = ["platform-root"]` on its `devd dtb launch` entry.

## Tests

Host matrix (`cargo nextest run -p raven-init`, `grants_ut.rs`):

* unprivileged service denied both device grants;
* `pcid` PCI root allowed, unchanged rights;
* `devd` platform root allowed only with the declared capability;
* prefixed names (`pcid-helper`, `devd-helper`) denied;
* `devd` without the capability gets no device handle;
* `platform-root` combined with any other grant is denied;
* the release and server `devd` manifests parse and declare no `platform-root`;
* the Pi 4 qtest fixture runs under `name = "devd"` and declares `platform-root`.

Integration (`cargo xtask qtest --case pi4-devd --arch aarch64`): `devd` runs under its real
service name with `platform-root`, creates the fake driver's BAR/DMA/IRQ handles and launches it.
`cargo xtask qtest --case release-login --arch x86_64` proves the release image is unchanged.

## Non-goals

* No `crates/raven-abi`, contract-crate or kernel changes.
* No PCI root change: `pcid` keeps `root` with the same rights.
* No arbitrary-service root grant and no ambient device capability for spawned drivers.
