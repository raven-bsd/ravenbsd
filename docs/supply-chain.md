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

The release build uses a `cargo vendor` snapshot with checksums and `source.crates-io.replace-with`,
built `--offline`. If the snapshot is too large to commit, it is a checksummed release artifact
referenced by the release evidence. The snapshot is unmodified upstream code; it is not a fork.

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

`scripts/release-vendor.sh` creates the snapshot and its evidence:

```text
vendor/                 74 registry crates, 47 MiB, cargo checksums per crate
vendor/manifest.toml    lock sha256 ccf2406f…, 74 crates, SBOM sha256 79b53b6e…
vendor/sbom.cdx.json    CycloneDX 1.5, 74 components with SHA-256 hashes and licences
.cargo/config.toml      source.crates-io.replace-with = "vendored-sources"
```

The manifest and SBOM are generated only by the script; the snapshot is committed so a clean
checkout builds with no crates.io access:

```sh
timeout 360 sh scripts/release-vendor.sh
timeout 300 cargo xtask image --arch x86_64 --profile release \
  --output build/release/vendor-check.img --offline
```

`.cargo/audit.toml` is a symlink to the reviewed root `audit.toml`; `cargo audit` reads that path.
`deny.toml` carries the same advisory exceptions plus the bans/licences/sources policy.

### Audit exceptions

All five current advisories are name/version collisions with the first-party workspace utilities
`userland/bin/tar` and `userland/bin/time` (both `0.1.0`, `publish = false`). No upstream
`tar-rs`/`time` crate is linked or shipped. SC2 renames the utilities to `raven-tar`/`raven-time`;
these exceptions expire 2027-02-28. No blanket ignore.

| ID | Owner | Reachability | Expiry |
|---|---|---|---|
| RUSTSEC-2018-0002 | kevin | first-party `tar` utility; no tar-rs code | 2027-02-28 |
| RUSTSEC-2021-0080 | kevin | first-party `tar` utility; no tar-rs code | 2027-02-28 |
| RUSTSEC-2026-0067 | kevin | first-party `tar` utility; no tar-rs code | 2027-02-28 |
| RUSTSEC-2026-0068 | kevin | first-party `tar` utility; no tar-rs code | 2027-02-28 |
| RUSTSEC-2020-0071 | kevin | first-party `time` utility; no time crate code | 2027-02-28 |

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
