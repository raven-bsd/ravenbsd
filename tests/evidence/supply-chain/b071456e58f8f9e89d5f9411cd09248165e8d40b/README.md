<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K390 supply-chain evidence

Tree `b071456e` (K390 branch). Host-side supply-chain records for the vendor artifact and the
`cargo-deny` gate.

| Artifact | What it is |
|---|---|
| `vendor-build.log` | `sh scripts/release-vendor.sh`; 771 crates, 131 MiB tarball, exit 1 by design because `artifact_url = "UNPUBLISHED"` |
| `vendor-local.log` | `sh scripts/release-vendor.sh --local`; tarball SHA-256 matches the manifest |
| `cargo-deny.log` | `cargo deny check`; `advisories ok, bans ok, licenses ok, sources ok` (321 internal path-wildcard warnings) |
| `vendor-manifest.toml` | copy of the generated manifest (URL intentionally UNPUBLISHED) |
| `tarball.sha256` | SHA-256 of `build/release/vendor-full.tar.zst`; the tarball itself is not committed or uploaded |
| `SHA256SUMS` | hashes of these records |

The only licence exception is `ucs2 0.3.3` MPL-2.0 via `uefi`/`raven-loader`, owner kevin,
expiry 2027-02-28; it is crate-specific in `deny.toml` and documented in `audit.toml`.
Advisory exceptions are empty. No blanket ignore.
