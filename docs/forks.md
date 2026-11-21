<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Fork maintenance register

Status: **OBSERVED** for the commits and paths below; **PROPOSED** for the future sync dates and
exit plans. Governing decision: DR-64. Policy: `docs/supply-chain.md`; runbook:
`ravenbsd-prjmgmt/process/SUPPLY-CHAIN.md`.

A fork is in-tree source with RavenBSD patches. An unmodified crate pinned by `Cargo.lock` is a
dependency, not a fork. Every fork row names an owner, upstream, commit, patch queue, sync cadence,
CVE watch, ABI/behaviour test and exit plan.

## Register

| Fork | Upstream and commit | Local patches | Owner | Last / next sync | CVE watch | ABI/behaviour test | Exit plan |
|---|---|---|---|---|---|---|---|
| Target libc shim and Rust std (`toolchain/vendor/libc`, `toolchain/patches`) | `rust-lang/libc` 0.2.186, registry git SHA `42620ffc4109dc32e02f1cae9e63a3f4311b4b71`; donor checkout `7631ff2ae852bdef359933b65349773f58733ece`; Rust toolchain version in `toolchain/rust-version` | `0001-add-ravenbsd-targets.patch`, `0002-libc-ravenbsd.patch`, `0003-posix-spawn-ravenbsd.patch`; K158 pthread-size correction in all three libc copies | kevin (build/toolchain) | 2026-11-19 / 2027-02-28 | RustSec `cargo audit` + `cargo deny advisories`; upstream libc and Rust security releases | `tests/toolchain/pthread-abi/check.sh` compares the ABI sizes from `raven-libc-abi` against `toolchain/vendor/libc`, the patch's Raven module and `userland/uutils/libc-ravenbsd`; host build and the K158 `mpsc-wake` runtime gate | Upstream first: send the RavenBSD target and pthread sizes to `rust-lang/libc`/Rust. If accepted, replace the vendor/patch set with the released upstream; otherwise regenerate the target module from `raven-libc-abi` and keep only the target-selection patches. Retire when the pinned toolchain no longer needs a local std patch. |
| uutils coreutils (`userland/uutils`) | `uutils/coreutils` commit `cf030b82ef519ab60ccacf763f7f086dd612d9c3` (0.13.0 workspace), MIT | RavenBSD target/libc adaptations and the project build overlay under `userland/uutils`; no upstream `.git` history is kept in tree | kevin (userspace/build) | 2026-11-19 / 2027-02-28 | RustSec advisory feed for the uutils dependency graph; upstream uutils security releases | `make check-fast`; the `base-tools`/`base-tools-du` qtests; `cargo nextest` for the uutils host tests where the target allows | Upstream the RavenBSD target and the minimal libc shims. When an upstream release builds for the target, drop the local copy and pin the release. If uutils drops a needed utility or stalls, keep the last good commit with a named owner and re-evaluate at the quarterly review. |
| brush (`userland/brush`) | `reubeno/brush` commit `8adb6bc06815c34ccf6503902f6f292e04b1cbba`, MIT | RavenBSD target/libc and shell-profile adaptations in the workspace; the upstream MIT notices are kept per file | bob (compatibility/userland) | 2026-11-19 / 2027-02-28 | RustSec advisory feed for the brush workspace; upstream brush security releases | brush's own parser/execution tests as vendored; the shell qtests and `make check-fast`; REPL/script behaviour is exercised by the shell fixtures | Upstream the target support; replace the in-tree copy when an upstream release builds and passes the shell fixtures. If brush diverges too far, freeze the last compatible commit and keep a compatibility shim rather than patching the parser/executor indefinitely. |
| Frozen `raven-rmm` and target-libc provenance (`crates/raven-rmm`, libc RavenBSD module) | `redox-os/rmm` commit `015af335c7bed8bfaa8c2ae8419ceba46fe0a227`, MIT, frozen by DR-53; target-libc provenance from `rust-libc` `7631ff2ae852bdef359933b65349773f58733ece` / libc 0.2.186 | The Redox `rmm` files are frozen provenance: no new feature patches; only security or correctness fixes that cannot wait for the replacement. The RavenBSD libc module is maintained with the target-libc row above | mark (allocator/memory) for `raven-rmm`; kevin for the target-libc module | 2026-11-09 frozen / 2027-02-28 review | RustSec advisory checks still cover the crate; Redox upstream is retired as a donor, so no sync is expected | `cargo nextest -p raven-rmm`; the allocator/memory suites; `tests/toolchain/pthread-abi/check.sh` for the target-libc half; boot/soak tests for the page-table use | Replace `raven-rmm` with the first-party `raven-vmem`/`raven-kmem`/pager implementation under DR-53, then delete the frozen copies. The target-libc provenance retires with the Rust target upstreaming exit above. No new Redox dependency is allowed. |

## Patch queue and sync rules

1. `toolchain/patches` is ordered and applied to the pinned Rust/libc source; a new patch needs the
   upstream issue/PR link when one exists, and a row here when it changes ABI or target selection.
2. `userland/uutils` and `userland/brush` are independent cargo workspaces; their locks are part of
   the release-vendor snapshot (`scripts/release-vendor.sh`, `vendor/manifest.toml`).
3. Quarterly review: rebase the fork onto the upstream commit, run the ABI/behaviour tests, and
   record the new commit and date in this table. A fork that misses a security fix is a finding.
4. No silent divergence: every local patch has an owner and an exit plan. No blanket advisory
   ignore; an exception needs owner, reachability evidence and expiry (`audit.toml`/`deny.toml`).
5. K158 is the model: the vendored libc pthread sizes had drifted (32/12/8 vs 56/40/48); the
   three-copy `pthread-abi` check now runs in the host gate so that divergence cannot recur
   silently.
