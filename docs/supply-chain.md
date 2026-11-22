<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Supply-chain policy

Status: ACTIVE, 2026-11-19. Governing decision: DR-64. Operational runbook:
`ravenbsd-prjmgmt/process/SUPPLY-CHAIN.md`. Tickets: SC1 (vendored release/audit), SC2
(fork register/naming), SC3 (weekly dependency loop).

## 1. Scope

This policy covers every third-party Rust crate, the patched Rust toolchain/std, vendored source
copies and in-tree forks used by RavenBSD. It applies to the base OS, the release images, the
toolchain and the build/test infrastructure. It does not change the licence rules in
`architecture/12-licensing-and-provenance.md`.

## 2. Principles

1. **Own the build input, not the world.** `Cargo.lock` pins versions; a vendored graph guarantees
   availability; a small fork set gives control. Forking every crate makes us their maintainer and
   removes upstream security fixes.
2. **A dependency is a maintenance obligation only when it is shipped and reachable.** Dev-only,
   optional and unused crates are tracked, not treated as release emergencies.
3. **Upstream-first.** Prefer an upstream fix. Fork when upstream is dead/unresponsive or the
   change is deep. No silent divergence.
4. **No known reachable critical/high in a shipped binary.** That is the release gate, not "zero
   advisories".
5. **Every exception has an owner, a reason, reachability evidence and an expiry.** No blanket
   ignores.
6. **Reproducible and offline for releases.** A missing vendored crate is a build failure, not a
   network fetch.
7. **Fewer dependencies is a security feature.** Remove unused crates and duplicate versions;
   disable unused features; prefer std.

## 3. Roles

| Role | Responsibility |
|---|---|
| Supply-chain owner (build/toolchain lane) | Vendor snapshot, audit/deny/SBOM, the weekly loop, the fork register, CVE co-ordination |
| Fork owner | Patch queue, upstream sync, CVE watch, ABI/behaviour tests, exit plan for one fork |
| Reviewer | Reviews new dependency intake, fork divergences and every exception |
| Owner | Accepts any release-blocking exception in writing |

## 4. Inventory and SBOM

* `Cargo.lock` is committed. The shipped graph, dev graph and optional/feature graph are measured
  separately.
* Every release produces an SBOM (CycloneDX and/or `cargo auditable`) and records it with the
  release evidence.
* The inventory records direct/transitive counts, duplicate versions, features/targets, licences,
  `build.rs`/native code and the fork register.

## 5. Dependency intake

A new dependency needs a ticket or an explicit review note that records:

* the need and why std/base cannot provide it;
* licence (permissive allow-list only; no GPL/AGPL in the base, CDDL only in separate add-ons);
* maintenance status and upstream activity;
* `build.rs`/native/`unsafe` audit;
* the shipped graph impact (features, duplicates, transitive count);
* a removal/exit plan.

No unknown registries, git dependencies or unverified sources. Checksums are mandatory.

## 6. Update cadence

| Cadence | Action |
|---|---|
| Continuous | `cargo-audit`/`cargo-deny`/OSV, SBOM and the dead-crate checker in CI |
| Weekly | One dependency-audit report: shipped-vs-dev, duplicates, unused, update dry-run, decision queue |
| Monthly | Batched semver-compatible updates on a branch with the full gates; major updates per release |
| Per release | Vendored snapshot, SBOM, audit gate, fork-register review |
| Quarterly | Fork sync/exit review, dead-crate decisions, licence/source review |

Toolchain and MSRV are pinned (`rust-toolchain.toml`); a dependency that needs a newer stable than
the pin is rejected or handled by an explicit toolchain ticket. `cargo update` is never run
automatically on a release branch.

## 7. Vulnerability response

### 7.1 Release impact

| Severity | Shipped and reachable | Shipped, not reachable / dev-only |
|---|---|---|
| Critical | **Blocks release**; hotfix lane; owner approval required to ship without a fix | Tracked; patched in the hotfix or next release |
| High | **Blocks release** unless the owner accepts it in writing with reachability evidence | Tracked; patch in the next release |
| Medium | Fix before the next scheduled release; tracked | Tracked |
| Low | Tracked; batched | Tracked |

Time-to-remediate is **measured** (p50/p90) and reported; it is not promised in human units.

### 7.2 Triage

1. Is the crate in the shipped graph (not dev-only, optional or unused)?
2. Is the vulnerable function reachable in our build, features and target?
3. What is the exposure (network, privileged, local, data path)?
4. Fix path: upgrade -> upstream patch -> local fork patch -> disable feature/replace.
5. If no fix exists: owner-approved exception with reachability evidence and an expiry date.

Rust does not yet have a mature automatic reachability checker; the analysis is feature/target/
call-path based and is recorded in the exception. Do not treat a scanner hit as proof either way.

### 7.3 Security hotfix lane

A critical advisory in a shipped binary may bypass a feature freeze with owner approval, but not
the security-relevant tests:

1. Advisory received; same-day triage and reachability.
2. Minimal patch: upgrade, local fork patch, or disable the feature.
3. Targeted tests, SBOM update, image rebuild, normal `make check` where feasible.
4. Release note/advisory entry with the affected versions and the fix.
5. Full soak follows; the exception and the soak result are recorded. A known reachable
   critical/high is never waived silently.

### 7.4 Exceptions

Every exception is a row in the audit/deny configuration with: advisory ID, crate/version, owner,
reason, reachability evidence, compensating control, expiry/review date. Expired exceptions fail
the audit.

## 8. Vendoring and forks

### 8.1 Vendored graph

The release build uses a `cargo vendor` snapshot with checksums and an **opt-in**
`source.crates-io.replace-with` in `.cargo/vendor.toml`, built `--offline`. Ordinary and clean
builds resolve from crates.io; a fresh checkout without the untracked `vendor/` tree builds
normally. If the snapshot is too large to commit, it is a checksummed release artifact referenced
by the release evidence. The snapshot is unmodified upstream code; it is not a fork.

### 8.2 Forks and adoptions

Fork only when we patch the crate or upstream is dead/critical. Adopt only when upstream is dead
and the crate is critical. Each fork has:

* upstream URL and commit, version, licence;
* local patch list with an upstream issue/PR link where one exists;
* owner, last sync, next review, CVE watch;
* ABI/behaviour tests (a fork without a test is a finding);
* exit plan (upstream the patches, replace the crate, or adopt permanently).

Current forks/frozen copies: `toolchain/vendor/libc` + `toolchain/patches` (target libc/std;
K158's pthread ABI drift is the model for a fork check), `userland/uutils`, `userland/brush`, and
the frozen `crates/raven-rmm` / target-libc provenance (S117).

## 9. Licences and provenance

Permissive licences only in the base: MIT, Apache-2.0, BSD-2/3/0, ISC, Zlib, Unicode and the
documented exceptions in `architecture/12-licensing-and-provenance.md`. No GPL/AGPL/proprietary in
the base; CDDL only in separate add-on processes. Every copied file keeps its notice and a
`PROVENANCE.toml` entry. Licence checks are per file, not per repository.

## 10. Build and release

* Release images build offline from the vendor snapshot; a missing crate fails the build.
* The SBOM, audit result (with exceptions), fork-register review and vendor manifest are release
  evidence.
* The release gate is: no reachable critical/high in shipped binaries; every exception unexpired
  and owner-approved; the SBOM matches the image.
* A CVE fix that changes the image invalidates the old image hash and requires a fresh image/gate
  record; it does not reuse the previous evidence.

### SC1 vendored snapshot (2026-11-19)

`scripts/release-vendor.sh` creates the snapshot and its evidence. The release graph is three
cargo workspaces — the root product, `userland/brush` and `userland/uutils` — merged into one
`vendor/` tree:

```text
vendor/                 771 crates, 1.2 GiB, cargo checksums per crate (not tracked source)
vendor/manifest.toml    root/brush/uutils lock sha256, 771 crates, SBOM sha256 80c2994a…
vendor/sbom.cdx.json    CycloneDX 1.5, 771 components with SHA-256 hashes and licences,
                        sha256 1f89f0edbddf41aa67bbab1cde24ee92a0a90b2a48954c48d5120d4e22662c47
build/release/vendor-full.tar.zst  full snapshot artifact, 131 MiB,
                                   sha256 ae2a44825d6d25e594501931cdd12b031b52e95d2579ff76ad89bc7b3f92bac8
.cargo/vendor.toml      opt-in source.crates-io.replace-with = "vendored-sources"
.cargo/config.toml      aliases and target cfgs only (normal builds use crates.io)
```

The 1.2 GiB snapshot is too large to track and must not receive the project SPDX headers: only
`vendor/manifest.toml` and `vendor/sbom.cdx.json` are committed. The snapshot is a checksummed
release artifact (`vendor-full.tar.zst`); upload it to the release evidence store and record the
URL with the manifest checksum. Materialize and verify it locally on a fresh checkout (no publish URL required):

```sh
timeout 900 sh scripts/release-vendor.sh   # builds; exits 1 with artifact_url = "UNPUBLISHED"
timeout 60  sh scripts/release-vendor.sh --local   # checks the tarball SHA-256 against the manifest
```

### Publish and local verification (K390)

`vendor/manifest.toml` records `artifact_url = "UNPUBLISHED"` until the owner publishes the
artifact. A release build without a URL fails closed; there is no placeholder store URL.

```sh
# owner: upload build/release/vendor-full.tar.zst, then record the URL
timeout 900 sh scripts/release-vendor.sh --url 'https://<release-store>/vendor-full.tar.zst'
# or: RELEASE_VENDOR_URL='https://...' timeout 900 sh scripts/release-vendor.sh

# offline/CI: verify the local tarball without any URL
timeout 60 sh scripts/release-vendor.sh --local
```

The local verification prints the SHA-256 and fails if it differs from the manifest. The tarball is
not committed and is not uploaded by the script.

`cargo deny check` is the enforcement command (cargo-deny 0.22.0; the tool is pinned by the
release host, not by Cargo). `deny.toml` keeps the advisory list empty, denies unknown
registries/git, and allows only the reviewed `ucs2` MPL-2.0 exception; wildcard internal path
dependencies are a warning with the owner/reason/expiry recorded in `audit.toml`.

```sh
CARGO_NET_OFFLINE=true \
CARGO_SOURCE_CRATES_IO_REPLACE_WITH=vendored-sources \
CARGO_SOURCE_VENDORED_SOURCES_DIRECTORY="$PWD/vendor" \
  timeout 300 cargo --offline --config .cargo/vendor.toml \
    xtask image --arch x86_64 --profile release --output build/release/vendor-check.img
```

Note the offline flag is a **cargo** flag (`cargo --offline xtask ...`); a trailing `--offline` is
an xtask argument and is rejected. `xtask` spawns nested `cargo build` processes that do not
inherit `--config`; the `CARGO_SOURCE_*` environment carries the same vendored source to them.

`.cargo/audit.toml` is a symlink to the reviewed root `audit.toml`; `cargo audit` reads that path.
`deny.toml` carries the same advisory exceptions plus the bans/licences/sources policy.

### Audit and licence exceptions

The advisory exception list is **empty**: SC2 renamed the first-party utilities to
`raven-tar`/`raven-time` (`publish = false`), so the former name-collision advisories do not
match and `audit.toml`/`deny.toml` carry `ignore = []`. No upstream `tar-rs`/`time` crate is
linked or shipped.

One reviewed licence exception remains:

| Crate | Licence | Owner | Reason / reachability | Expiry |
|---|---|---|---|---|
| `ucs2` 0.3.3 | MPL-2.0 | kevin | unmodified crates.io helper via `uefi` -> `raven-loader`; file-level copyleft, no Raven code derived | 2027-02-28 |

`cargo deny check` (0.22.0) is the enforcement command. Internal wildcard path dependencies are a
warning, not a release-blocking deny, because 321 workspace members are not marked
`publish = false`; registry/git unknown sources remain denied and the weekly audit tracks any
registry wildcard. The owner/reason/expiry is also in `audit.toml`. No blanket ignore.

## 11. Metrics

The dashboard tracks:

* shipped/direct/transitive dependency counts, duplicate versions, vendor coverage;
* open advisories by severity, and **reachable** critical/high in shipped binaries (target 0);
* unmaintained crates with a recorded decision / total;
* time from advisory to patched release candidate (p50/p90);
* stale forks: patch-queue age vs upstream, and forks with no ABI/behaviour test.

## 12. Large dependency graphs

For projects with hundreds or thousands of crates (the owner's other project is ~1500):

1. Measure the **shipped** graph first; dev/optional dependencies usually dominate the count.
2. Remove unused crates and duplicate versions; disable unused features; check feature
   unification.
3. Rank by shipped/reachable/severity; do not chase "latest" for every crate.
4. Give every dead/unmaintained crate one decision: remove, replace, fork/adopt, or keep-pinned
   with an owner and review date.
5. Automate the weekly inventory, advisory scan and update PRs; reserve human/owner attention for
   reachability, forks and critical exceptions.
6. Vendor the graph for availability; keep a private registry mirror if the build fleet needs it.
7. Measure time-to-remediate and reachable critical/high, not "number of outdated crates".

A raw "N dead crates" count without decisions is not a maintenance plan.

## 13. Non-goals

No blanket forks, no private patching without a register row, no network in the release build, no
GPL/AGPL in the base, and no claim that every advisory can be fixed instantly. The guarantee is a
fast, auditable path from detection to a patched release, with no known reachable critical/high
shipped silently.
