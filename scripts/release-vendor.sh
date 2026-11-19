#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# SC1: build the vendored release graph, its CycloneDX SBOM and the manifest.
#
# The release image and every `--offline` build use `.cargo/config.toml`'s
# `source.crates-io.replace-with = "vendored-sources"`. This script is the only writer of the
# snapshot; it pins to Cargo.lock, keeps cargo's per-crate `.cargo-checksum.json`, and records the
# lock/SBOM hashes. A missing crate fails the vendor command; it is never fetched silently by a
# release build.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

vendor_dir=vendor
lock=Cargo.lock
sbom=$vendor_dir/sbom.cdx.json
manifest=$vendor_dir/manifest.toml
check=0
case "${1:-}" in
    --check) check=1 ;;
    -h|--help)
        echo "usage: sh scripts/release-vendor.sh [--check]"
        exit 0
        ;;
    "")
        ;;
    *)
        echo "release-vendor: unknown argument: $1" >&2
        exit 2
        ;;
esac

command -v cargo >/dev/null 2>&1 || { echo "release-vendor: cargo not found" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "release-vendor: python3 not found" >&2; exit 1; }
[ -f "$lock" ] || { echo "release-vendor: $lock is missing" >&2; exit 1; }

verify_layout() {
    [ -d "$vendor_dir" ] || { echo "release-vendor: $vendor_dir is missing" >&2; exit 1; }
    [ -f "$vendor_dir/.cargo-checksum.json" ] || true
    grep -q 'replace-with = "vendored-sources"' .cargo/config.toml || {
        echo "release-vendor: .cargo/config.toml has no vendored-sources replacement" >&2
        exit 1
    }
    grep -q "directory = \"$vendor_dir\"" .cargo/config.toml || {
        echo "release-vendor: .cargo/config.toml does not point at $vendor_dir" >&2
        exit 1
    }
    [ -f "$sbom" ] || { echo "release-vendor: $sbom is missing" >&2; exit 1; }
    [ -f "$manifest" ] || { echo "release-vendor: $manifest is missing" >&2; exit 1; }
    [ -f audit.toml ] || { echo "release-vendor: audit.toml is missing" >&2; exit 1; }
    [ -f deny.toml ] || { echo "release-vendor: deny.toml is missing" >&2; exit 1; }
    echo "release-vendor: layout ok ($vendor_dir)"
}

if [ "$check" -eq 1 ]; then
    verify_layout
    exit 0
fi

mkdir -p build/release
echo "==> vendor: cargo vendor --locked --versioned-dirs $vendor_dir"
cargo vendor --locked --versioned-dirs "$vendor_dir" >build/release/vendor-config.txt

echo "==> metadata: cargo metadata --locked --offline"
cargo metadata --locked --offline --format-version 1 >build/release/vendor-metadata.json

echo "==> SBOM: $sbom"
python3 - "$lock" build/release/vendor-metadata.json "$sbom" <<'PY'
import datetime
import json
import os
import sys

lock_path, metadata_path, sbom_path = sys.argv[1:4]
with open(metadata_path, "r", encoding="utf-8") as handle:
    metadata = json.load(handle)
licenses = {
    package["name"] + "@" + package["version"]: package.get("license")
    for package in metadata.get("packages", [])
}

# Cargo.lock is a simple TOML shape and the release host may have an older Python without
# tomllib. Parse only the package name/version/source/checksum fields we need.
lock_packages = []
current = None
for raw in open(lock_path, "r", encoding="utf-8"):
    line = raw.strip()
    if line == "[[package]]":
        if current:
            lock_packages.append(current)
        current = {}
        continue
    if current is None:
        continue
    for key in ("name", "version", "source", "checksum"):
        prefix = key + " = \""
        if line.startswith(prefix) and line.endswith("\""):
            current[key] = line[len(prefix):-1]
if current:
    lock_packages.append(current)

components = []
for package in lock_packages:
    source = package.get("source", "")
    checksum = package.get("checksum")
    if "registry+https://github.com/rust-lang/crates.io-index" not in source or not checksum:
        continue
    component = {
        "type": "library",
        "name": package["name"],
        "version": package["version"],
        "purl": "pkg:cargo/" + package["name"] + "@" + package["version"],
        "hashes": [{"alg": "SHA-256", "content": checksum}],
    }
    license = licenses.get(package["name"] + "@" + package["version"])
    if license:
        component["licenses"] = [{"expression": license}]
    components.append(component)
components.sort(key=lambda item: (item["name"], item["version"]))

epoch = os.environ.get("SOURCE_DATE_EPOCH")
timestamp = (
    datetime.datetime.fromtimestamp(int(epoch), datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    if epoch
    else datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
)
bom = {
    "bomFormat": "CycloneDX",
    "specVersion": "1.5",
    "version": 1,
    "metadata": {
        "timestamp": timestamp,
        "component": {"type": "application", "name": "ravenbsd-release", "version": "0.1.0"},
        "tools": [{"vendor": "RavenBSD", "name": "release-vendor", "version": "1"}],
    },
    "components": components,
}
with open(sbom_path, "w", encoding="utf-8") as handle:
    json.dump(bom, handle, sort_keys=True, separators=(",", ":"))
    handle.write("\n")
print(f"release-vendor: SBOM components={len(components)}")
PY

echo "==> manifest: $manifest"
python3 - "$lock" "$vendor_dir" "$sbom" "$manifest" <<'PY'
import datetime
import hashlib
import os
import sys

lock_path, vendor_dir, sbom_path, manifest_path = sys.argv[1:5]
lock_hash = hashlib.sha256(open(lock_path, "rb").read()).hexdigest()
sbom_hash = hashlib.sha256(open(sbom_path, "rb").read()).hexdigest()
crates = 0
for entry in os.listdir(vendor_dir):
    path = os.path.join(vendor_dir, entry)
    if os.path.isdir(path) and os.path.isfile(os.path.join(path, ".cargo-checksum.json")):
        crates += 1
epoch = os.environ.get("SOURCE_DATE_EPOCH")
timestamp = (
    datetime.datetime.fromtimestamp(int(epoch), datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    if epoch
    else datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
)
with open(manifest_path, "w", encoding="utf-8") as handle:
    handle.write("# SPDX-License-Identifier: BSD-3-Clause\n")
    handle.write("# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n")
    handle.write("# Generated by scripts/release-vendor.sh; do not edit by hand.\n\n")
    handle.write(f'generated_utc = "{timestamp}"\n')
    handle.write(f'lock_sha256 = "{lock_hash}"\n')
    handle.write(f'vendored_crates = {crates}\n')
    handle.write('vendor_directory = "vendor"\n')
    handle.write(f'sbom = "{sbom_path}"\n')
    handle.write(f'sbom_sha256 = "{sbom_hash}"\n')
print(f"release-vendor: vendored_crates={crates} lock_sha256={lock_hash}")
PY

verify_layout
echo "release-vendor: PASS"
