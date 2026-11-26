# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# Fork and adoption register

Status: ACTIVE, 2026-11-21 (SC2). Authority: DR-64. Policy:
[`supply-chain.md`](supply-chain.md) section 8. Operational runbook:
`ravenbsd-prjmgmt/process/SUPPLY-CHAIN.md`.

Every fork or adoption has one row: upstream and commit, local patches, owner,
last and next sync, CVE watch, an ABI/behaviour test (a fork without a test is a
finding) and an exit plan. Upstream-first: fork only when we patch, or when
upstream is dead and the crate is critical.

## Register

| Fork / adoption | Upstream and commit (licence) | Local patches | Owner | Last sync | Next review | CVE watch | ABI / behaviour test | Exit plan |
|---|---|---|---|---|---|---|---|---|
| `toolchain/patches` — patched Rust std (`x86_64-unknown-ravenbsd`, draft aarch64/riscv64) | rust-lang/rust stable `1.97.1`, source tarball (installed `rustc 1.97.1 (8bab26f4f 2026-07-14)`); (MIT/Apache-2.0) | `0001-add-ravenbsd-targets.patch`, `0002-libc-ravenbsd.patch`, `0003-posix-spawn-ravenbsd.patch`; `toolchain/bootstrap.toml`, `toolchain/build.sh` | kevin (build/toolchain lane) | 2026-11-12 (K19 end-to-end `make toolchain`) | 2027-02-15 (quarterly, or each Rust stable bump) | RustSec/OSV via the weekly SC3 loop; `cargo-audit` on the graph | `tests/toolchain/pthread-abi/check.sh` (K158 model), `toolchain/audit-errno.py`, `make toolchain`, `cargo xtask libc-pack` | Upstream the `ravenbsd` target to rust-lang and shrink the series to the target registration; otherwise keep the three-patch queue pinned to the stable channel and re-derive on each stable release |
| `toolchain/vendor/libc` — vendored `libc` 0.2.186 with the ravenbsd unix module | crates.io `libc 0.2.186` archive (MIT), module from rust-lang/rust `library/libc-ravenbsd` @ `8bab26f4f68e0e26f0bb7960be334d5b520ea452` | K19 ravenbsd module graft; registry/patch identity in `toolchain/libc-0.2.186-provenance.md` | kevin | 2026-11-15 (K26a) | 2027-02-15 (quarterly, or each `libc` release) | RustSec/OSV (libc advisories are upstream-tracked); weekly SC3 loop | `tests/toolchain/pthread-abi/check.sh` cross-checks this copy against `raven-libc-abi` and the `0002` patch; `cargo xtask libc-pack`; the brush/uutils graphs build against it | Upstream the ravenbsd module into `libc`/rust so the graft disappears; the graft is the only local delta and must be re-applied (not re-invented) on each release |
| `userland/uutils` — vendored uutils coreutils 0.13.0 workspace | uutils/coreutils @ `cf030b82ef519ab60ccacf763f7f086dd612d9c3` (MIT) | `src/uu/mv/src/mv.rs` repaired per upstream `cc1a8b2`; compatibility copies `nix 0.31.3` (`b5933ca1`), `rustix 1.1.6` (`4a563ad6`), `rlimit 0.11.0` (`839f3476`), each a cfg-arm port only | kevin (build/userland lane) | 2026-11-15 | 2027-02-15 (quarterly) | RustSec/OSV; the weekly SC3 loop; the tree is in `.provenance-ignore` with every upstream MIT header kept | The uutils crates' unit tests (nextest), `tests/posix/utility/**` behaviour rows, `tests/fixtures/required-utils/build.sh` image build | Upstream the `mv` fix and the ravenbsd cfg arms; drop unused utilities; exit = unmodified upstream uutils releases |
| `userland/brush` — pinned brush 0.4.0 workspace | brush @ `8adb6bc06815c34ccf6503902f6f292e04b1cbba` (MIT) | Vendored dependency crates (`mio`, `nix`, `rlimit`, …); the K26c libc delta needed by the graph is documented in `userland/brush/ravenbsd-libc-delta.md` (follow-up C64h); upstream fuzz/xtask/bench/e2e/docs omitted | kevin (build/userland lane) | 2026-11-14 | 2027-02-15 (quarterly) | RustSec/OSV; weekly SC3 loop | `tests/brush/bash-compat.sh`, the `brush-login` qtest, the brush crates' unit tests via nextest | Land the libc delta (C64h) in the toolchain libc or upstream; exit = unmodified upstream brush once the delta is gone |
| `crates/raven-rmm` + target-libc provenance — frozen | redox-kernel @ `015af335c7bed8bfaa8c2ae8419ceba46fe0a227` (MIT), frozen by S117/DR-63 | Trimmed to the S06 types and traits; `PROVENANCE.toml` import pins the revision | mark (allocator/memory lane) | 2026-11-09 | 2027-02-15 (quarterly; frozen means replace-on-touch) | RustSec/OSV; weekly SC3 loop | `cargo nextest run -p raven-rmm` model tests; the target-libc checks in `tests/toolchain/pthread-abi/check.sh` | Frozen per S117/DR-63: keep only what is proven and replace high-risk pieces when touched (DR-53); exit = a maintained allocator or an upstreamed subset |

## The K158 model for fork checks

K158 found that the three target-libc copies of the pthread object sizes and
alignments had drifted from the authoritative `raven-libc-abi` definitions.
`tests/toolchain/pthread-abi/check.sh` is the model every fork with an ABI
surface must follow: it extracts the ravenbsd module from
`toolchain/patches/0002-libc-ravenbsd.patch`, reads the sizes from
`libc/ravenlibc/crates/raven-libc-abi/src/generated/structs.rs`,
`toolchain/vendor/libc/src/unix/ravenbsd/mod.rs` and
`userland/uutils/libc-ravenbsd/src/unix/ravenbsd/mod.rs`, and compares them
(plus the thread `once`/`spin` internals) so a copy cannot silently diverge.
The check is host-only and part of the normal gates.

Rule: a fork patch that changes an ABI-visible definition must add or extend an
equivalent cross-check in the same ticket; the register row names it.

## Sync cadence and review

| Cadence | Action |
|---|---|
| Weekly | SC3 dependency loop: advisories, duplicates, unused crates, fork CVEs |
| Quarterly | Fork sync/exit review; every row re-approved or retired |
| Per release | Register review recorded with the release evidence (DR-64) |

A fork that falls behind upstream security fixes is itself a finding: sync or
retire it; do not carry stale forks.

## First-party naming (SC2)

The first-party workspace utilities `userland/bin/tar` and `userland/bin/time`
are packages `raven-tar` and `raven-time` (`publish = false`; their binaries stay
`tar`/`time`). This removed the `cargo-audit`/`cargo-deny` name collisions with
the upstream `tar-rs`/`time` advisories, so the five reviewed exceptions were
deleted from `audit.toml`/`deny.toml` and no blanket ignore remains. The
first-party crates are not forks: they are covered by their own tests and the
POSIX rows (`docs/posix/P4-archive.md`, `P4-simple2.md`).
