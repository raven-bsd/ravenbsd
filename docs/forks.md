<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Forks and frozen copies

Status: ACTIVE, 2026-11-21 (SC2). Governing decision: DR-64. Policy: `docs/supply-chain.md`;
process: `ravenbsd-prjmgmt/process/SUPPLY-CHAIN.md`.

Every fork or frozen copy has an owner, an upstream and commit pin, the local patch queue, a sync
cadence, a CVE watch, an ABI/behaviour test and an exit plan. When a pin cannot be established
from the repository or its committed evidence it is recorded as a gap; it is never invented.

| Fork / frozen copy | Upstream and licence | Pinned commit | Local patches | Owner | Sync / CVE | ABI/behaviour test | Exit plan |
|---|---|---|---|---|---|---|---|
| `toolchain/vendor/libc` + `toolchain/patches` | `rust-lang/libc` 0.2.186 (MIT OR Apache-2.0); `rust-lang/rust` 1.97.1 std (MIT OR Apache-2.0) | libc `42620ffc4109dc32e02f1cae9e63a3f4311b4b71` (crate `.cargo_vcs_info.json`); rustc `8bab26f4f68e0e26f0bb7960be334d5b520ea452` (`rustc -vV`, release 1.97.1) | `0001-add-ravenbsd-targets.patch`, `0002-libc-ravenbsd.patch`, `0003-posix-spawn-ravenbsd.patch` | kevin | per Rust release and libc release; RustSec weekly | K158 pthread-size ABI cross-check against the Raven ABI; target libc/std build + host suite | upstream the RavenBSD target when accepted; otherwise keep the reviewed queue |
| `userland/uutils` | `uutils/coreutils` 0.13.0 (MIT) | **gap**: the in-tree vendor records the version and the workspace lock SHA-256 `f23df29e4ee3ab6d77b8fc17b973f10e8b6697029c2f04ed0d4443d5b5dbefd5` (`vendor/manifest.toml`), not the upstream git SHA. The matching `.crate` is not in the local registry cache. | K25a RavenBSD target/getrandom wiring; no upstream changes to the selected utilities | kevin | imported 2026-11 (K25a); quarterly review, next 2027-02-28; RustSec weekly | selected-set build (`tests/fixtures/required-utils`) and the utility host tests | upstream the target patches; otherwise keep the vendor update cadence |
| `userland/brush` | `reubeno/brush` 0.4.0 (MIT) | **gap**: the in-tree vendor records the version and the workspace lock SHA-256 `a84c5c980b2e97b906a207d6f8a1745494812e09fed0c6e183bee7f925c1c88e` (`vendor/manifest.toml`), not the upstream git SHA. The matching `.crate` is not in the local registry cache. | K26a/K26c RavenBSD port; shell profile and smoke wiring | kevin | imported 2026-11 (K26a); quarterly review, next 2027-02-28; RustSec weekly | `shell-brush` qtest smoke (skipped with the C64f reason until the target binary lands) | upstream the port; otherwise keep the reviewed queue |
| `crates/raven-rmm` + target-libc provenance | `redox-kernel` `rmm` (MIT) | `015af335c7bed8bfaa8c2ae8419ceba46fe0a227` (`PROVENANCE.toml`) | trimmed to the x86-64 types and the Raven page API (S117/DR-63) | kevin | frozen; CVE watch on the next touch | page-table/ABI unit tests and the boot memory-map path | replace with a first-party implementation when the page layer is touched |

## 1. Pin gaps and the exact source needed

1. **uutils upstream commit.** Not recorded in the import commit (`bafd5199`), the tree, or the
   vendor manifest (which pins lock hashes). Needed from the SC1 vendor evidence
   (`build/release/vendor-full.tar.zst` manifest / SBOM) or the K25a import record.
2. **brush upstream commit.** Same shape: import `98454e85` records `brush 0.4.0` only; the vendor
   manifest pins the workspace lock SHA-256. Needed from the SC1 vendor evidence or K26a.

Until those pins are recovered, the version + upstream URL + lock checksum are the reproducible
identity of the vendored tree; the register must not claim a git SHA it cannot prove.

## 2. Fork-check model

K158's pthread-size divergence is the model: a fork is accepted only with an ABI/behaviour check
that compares the fork's values with the Raven ABI (not with an assumption). Every register row
names its check; a fork without a check is a finding under `docs/supply-chain.md` section 8.2.

## 3. Review cadence

Quarterly: sync decision, patch-queue age, CVE status, exit-plan review. Weekly: RustSec and
cargo-deny across the shipped graph. Per release: register review plus the vendored snapshot and
SBOM evidence (DR-64).
