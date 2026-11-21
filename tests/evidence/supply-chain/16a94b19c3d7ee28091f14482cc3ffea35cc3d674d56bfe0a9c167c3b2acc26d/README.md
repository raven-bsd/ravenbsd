<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K390 supply-chain evidence — vendor artifact `16a94b19…`

Base `origin/master` `f469b2fda2fae9ea0bffe9f74827490c8cd580fb`; assembled 2026-11-21.

## Vendor artifact

| Field | Value |
|---|---|
| Path | `build/release/vendor-full.tar.zst` (not uploaded, not committed) |
| Size | 136,689,822 bytes |
| SHA-256 | `16a94b19c3d7ee28091f14482cc3ffea35cc3d674d56bfe0a9c167c3b2acc26d` |
| Manifest | `vendor/manifest.toml` (771 vendored crates, `artifact_url = "UNPUBLISHED"`) |
| Manifest copy | `tests/evidence/supply-chain/16a94b19…/manifest.toml` sha256 `8453273287eaf1cf644287a86c76c8188fe5bc6d48566ddb2a49f06e8dce48d3` |

`artifact.sha256` records the hash in `sha256sum` form. The artifact was not uploaded and the tree
was not modified.

**Lock currency (open gap).** The existing artifact was built at SC1 (2026-11-19) from root lock
`ccf2406fd658…`; the current `Cargo.lock` is `f2b09d7822c4…` (the pfctl merge and later changes).
`--local` verifies the artifact checksum only. Before uploading for a release, rebuild the vendor
graph on the release commit (`sh scripts/release-vendor.sh`), then record the new artifact hash;
otherwise the offline build would miss the newer lock entries. This ticket uses the existing
artifact as instructed and records the mismatch instead of rewriting the manifest's lock hashes.

## Local verification (no URL)

```text
$ timeout 900 sh scripts/release-vendor.sh --local
release-vendor: local artifact sha256=16a94b19c3d7ee28091f14482cc3ffea35cc3d674d56bfe0a9c167c3b2acc26d match
exit 0
```

`local-check.log` sha256 `d1b0f042a9e43d642f5c4fef79e7fe608e810dc2dfe1cc86f839344a97bdb9b7`.

Negative check: the manifest's `artifact_sha256` was temporarily replaced with 64 zeros;

```text
$ sh scripts/release-vendor.sh --local
release-vendor: artifact sha256 mismatch
  expected: 0000…0000
  actual:   16a94b19…
exit 1
```

The real hash was restored and `--local` returned exit 0.

## Publish-state checks

```text
$ sh scripts/release-vendor.sh --reuse                 # no URL
… artifact_url = "UNPUBLISHED"
release-vendor: UNPUBLISHED: pass --url or RAVEN_VENDOR_ARTIFACT_URL for a release build
exit 1

$ sh scripts/release-vendor.sh --reuse --url https://example.invalid/vendor-full.tar.zst
release-vendor: PASS url=https://example.invalid/vendor-full.tar.zst
exit 0
```

The test URL was not kept: the manifest was regenerated with `UNPUBLISHED` afterwards. Any
non-https URL is rejected (exit 2).

## cargo-deny

| Field | Value |
|---|---|
| Tool | `cargo-deny 0.22.0` |
| Install | `timeout 900 cargo install cargo-deny --version 0.22.0 --locked` |
| Measured install | 247.11 s |
| Command | `timeout 900 cargo deny check` |
| Result | `advisories ok, bans FAILED, licenses ok, sources ok` (exit 2) |
| Log | `deny-check.log` sha256 `c75bf09606b9fe5c207c621c7e96af2db4cd2610129c7478dac44786a39b8a17` |

- `advisories`: pass.
- `licenses`: pass with the scoped `ucs2 0.3.3` MPL-2.0 exception in `deny.toml` (owner/reason/
  expiry in the file; owner sign-off is still required for the release tag).
- `sources`: pass.
- `bans`: **321 first-party wildcard path dependencies** in ~300 workspace crates that do not
  declare `publish = false`. `allow-wildcard-paths = true` is set, but cargo-deny only permits
  wildcard path edges for private crates. The finding is not suppressed; owner Bo Maryniuk, reason
  and fix in `deny.toml`/`docs/supply-chain.md`, expiry 2026-12-31. The gate stays red until the
  workspace metadata is fixed.

## Reproduce

```sh
sha256sum -c tests/evidence/supply-chain/16a94b19c3d7ee28091f14482cc3ffea35cc3d674d56bfe0a9c167c3b2acc26d/artifact.sha256
timeout 900 sh scripts/release-vendor.sh --local
timeout 900 cargo deny check
timeout 900 make check-fast
```

`make check-fast` passed at 217.77 s (`check-fast: PASSED`).
