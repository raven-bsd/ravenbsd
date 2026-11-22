#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# SC1: build the vendored release graph, its CycloneDX SBOM and the manifest.
#
# The offline release build uses the opt-in `.cargo/vendor.toml`
# (`source.crates-io.replace-with = "vendored-sources"`); ordinary builds resolve from crates.io.
# The release graph is three cargo workspaces: the root product, the in-tree brush shell and the
# in-tree uutils slice. Each is
# vendored with its own committed Cargo.lock; the parts are merged into one `vendor/` directory
# with cargo's per-crate `.cargo-checksum.json`. A missing crate fails the vendor command; it is
# never fetched silently by a release build.
#
# The full snapshot is too large to commit. The script records it as
# `build/release/vendor-full.tar.zst` with a SHA-256 in `vendor/manifest.toml`; upload that
# artifact to the release evidence store and record the URL alongside the checksum.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

vendor_dir=vendor
parts=build/release/vendor-parts
artifact=build/release/vendor-full.tar.zst
sbom=$vendor_dir/sbom.cdx.json
manifest=$vendor_dir/manifest.toml
check=0
local_check=0
vendor_url=${RELEASE_VENDOR_URL:-}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --check) check=1; shift ;;
        --local) local_check=1; shift ;;
        --url)
            [ "$#" -ge 2 ] || { echo "release-vendor: --url needs a value" >&2; exit 2; }
            vendor_url=$2
            shift 2
            ;;
        -h|--help)
            echo "usage: sh scripts/release-vendor.sh [--check | --local | --url URL]"
            echo "  --check  verify the vendor layout only"
            echo "  --local  verify build/release/vendor-full.tar.zst against the manifest SHA-256"
            echo "  --url    publish URL recorded in vendor/manifest.toml (or RELEASE_VENDOR_URL)"
            exit 0
            ;;
        *)
            echo "release-vendor: unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

export RELEASE_VENDOR_URL="$vendor_url"

command -v cargo >/dev/null 2>&1 || { echo "release-vendor: cargo not found" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "release-vendor: python3 not found" >&2; exit 1; }
[ -f Cargo.lock ] || { echo "release-vendor: Cargo.lock is missing" >&2; exit 1; }
[ -f userland/brush/Cargo.lock ] || { echo "release-vendor: brush Cargo.lock is missing" >&2; exit 1; }
[ -f userland/uutils/Cargo.lock ] || { echo "release-vendor: uutils Cargo.lock is missing" >&2; exit 1; }

verify_layout() {
    [ -d "$vendor_dir" ] || { echo "release-vendor: $vendor_dir is missing" >&2; exit 1; }
    grep -q 'replace-with = "vendored-sources"' .cargo/vendor.toml || {
        echo "release-vendor: .cargo/vendor.toml has no vendored-sources replacement" >&2
        exit 1
    }
    grep -q "directory = \"$vendor_dir\"" .cargo/vendor.toml || {
        echo "release-vendor: .cargo/vendor.toml does not point at $vendor_dir" >&2
        exit 1
    }
    [ -f "$sbom" ] || { echo "release-vendor: $sbom is missing" >&2; exit 1; }
    [ -f "$manifest" ] || { echo "release-vendor: $manifest is missing" >&2; exit 1; }
    [ -f audit.toml ] || { echo "release-vendor: audit.toml is missing" >&2; exit 1; }
    [ -f deny.toml ] || { echo "release-vendor: deny.toml is missing" >&2; exit 1; }
    echo "release-vendor: layout ok ($vendor_dir)"
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        echo "release-vendor: sha256sum or shasum is required" >&2
        return 1
    fi
}

if [ "$local_check" -eq 1 ]; then
    [ -f "$manifest" ] || {
        echo "release-vendor: $manifest is missing; run a build or restore the manifest" >&2
        exit 1
    }
    [ -f "$artifact" ] || {
        echo "release-vendor: $artifact is missing; materialize the snapshot first" >&2
        exit 1
    }
    recorded=$(sed -n 's/^artifact_sha256 = "\(.*\)"$/\1/p' "$manifest")
    [ -n "$recorded" ] || {
        echo "release-vendor: $manifest has no artifact_sha256" >&2
        exit 1
    }
    actual=$(sha256_file "$artifact") || exit 1
    if [ "$recorded" != "$actual" ]; then
        echo "release-vendor: LOCAL SHA MISMATCH recorded=$recorded actual=$actual" >&2
        exit 1
    fi
    echo "release-vendor: local artifact ok $artifact sha256=$actual"
    exit 0
fi

if [ "$check" -eq 1 ]; then
    verify_layout
    exit 0
fi

mkdir -p build/release
rm -rf "$parts"
mkdir -p "$parts/root" "$parts/brush" "$parts/uutils"

echo "==> vendor: root"
cargo vendor --locked --versioned-dirs "$parts/root" >build/release/vendor-config-root.txt
echo "==> vendor: brush"
cargo vendor --locked --versioned-dirs --manifest-path userland/brush/Cargo.toml "$parts/brush" \
    >build/release/vendor-config-brush.txt
echo "==> vendor: uutils"
cargo vendor --locked --versioned-dirs --manifest-path userland/uutils/Cargo.toml "$parts/uutils" \
    >build/release/vendor-config-uutils.txt

echo "==> merge: $vendor_dir"
rm -rf "$vendor_dir"
mkdir -p "$vendor_dir"
cp -a "$parts/root/." "$vendor_dir/"
cp -an "$parts/brush/." "$vendor_dir/"
cp -an "$parts/uutils/." "$vendor_dir/"

echo "==> metadata: cargo metadata --locked --offline"
cargo metadata --locked --offline --format-version 1 >build/release/vendor-metadata-root.json
cargo metadata --locked --offline --format-version 1 \
    --manifest-path userland/brush/Cargo.toml >build/release/vendor-metadata-brush.json
cargo metadata --locked --offline --format-version 1 \
    --manifest-path userland/uutils/Cargo.toml >build/release/vendor-metadata-uutils.json

echo "==> SBOM: $sbom"
python3 - "$sbom" \
    Cargo.lock build/release/vendor-metadata-root.json \
    userland/brush/Cargo.lock build/release/vendor-metadata-brush.json \
    userland/uutils/Cargo.lock build/release/vendor-metadata-uutils.json <<'PY'
import datetime
import json
import os
import sys

sbom_path = sys.argv[1]
pairs = list(zip(sys.argv[2::2], sys.argv[3::2]))

licenses = {}
components = {}
for _, metadata_path in pairs:
    with open(metadata_path, "r", encoding="utf-8") as handle:
        metadata = json.load(handle)
    for package in metadata.get("packages", []):
        key = package["name"] + "@" + package["version"]
        if package.get("license"):
            licenses[key] = package["license"]

# Cargo.lock is a simple TOML shape and the release host may have an older Python without
# tomllib. Parse only the package name/version/source/checksum fields we need.
for lock_path, _ in pairs:
    current = None
    packages = []
    for raw in open(lock_path, "r", encoding="utf-8"):
        line = raw.strip()
        if line == "[[package]]":
            if current:
                packages.append(current)
            current = {}
            continue
        if current is None:
            continue
        for key in ("name", "version", "source", "checksum"):
            prefix = key + ' = "'
            if line.startswith(prefix) and line.endswith('"'):
                current[key] = line[len(prefix):-1]
    if current:
        packages.append(current)
    for package in packages:
        source = package.get("source", "")
        checksum = package.get("checksum")
        if "registry+https://github.com/rust-lang/crates.io-index" not in source or not checksum:
            continue
        key = package["name"] + "@" + package["version"]
        component = {
            "type": "library",
            "name": package["name"],
            "version": package["version"],
            "purl": "pkg:cargo/" + package["name"] + "@" + package["version"],
            "hashes": [{"alg": "SHA-256", "content": checksum}],
        }
        if key in licenses:
            component["licenses"] = [{"expression": licenses[key]}]
        components[key] = component

ordered = [components[key] for key in sorted(components)]
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
        "tools": [{"vendor": "RavenBSD", "name": "release-vendor", "version": "2"}],
    },
    "components": ordered,
}
with open(sbom_path, "w", encoding="utf-8") as handle:
    json.dump(bom, handle, sort_keys=True, separators=(",", ":"))
    handle.write("\n")
print(f"release-vendor: SBOM components={len(ordered)}")
PY

echo "==> artifact: $artifact"
tar --zstd -cf "$artifact" -C "$vendor_dir" .

echo "==> manifest: $manifest"
python3 - Cargo.lock userland/brush/Cargo.lock userland/uutils/Cargo.lock \
    "$vendor_dir" "$sbom" "$artifact" "$manifest" <<'PY'
import datetime
import hashlib
import os
import sys

root_lock, brush_lock, uutils_lock, vendor_dir, sbom_path, artifact_path, manifest_path = sys.argv[1:8]

def sha256(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()

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
    handle.write(f'root_lock_sha256 = "{sha256(root_lock)}"\n')
    handle.write(f'brush_lock_sha256 = "{sha256(brush_lock)}"\n')
    handle.write(f'uutils_lock_sha256 = "{sha256(uutils_lock)}"\n')
    handle.write(f'vendored_crates = {crates}\n')
    handle.write('vendor_directory = "vendor"\n')
    handle.write(f'sbom = "{sbom_path}"\n')
    handle.write(f'sbom_sha256 = "{sha256(sbom_path)}"\n')
    handle.write(f'artifact = "{artifact_path}"\n')
    handle.write(f'artifact_sha256 = "{sha256(artifact_path)}"\n')
    handle.write(f'artifact_url = "{os.environ.get("RELEASE_VENDOR_URL") or "UNPUBLISHED"}"\n')
print(f"release-vendor: vendored_crates={crates} artifact_sha256={sha256(artifact_path)}")
PY

verify_layout
if [ -z "$vendor_url" ]; then
    echo "release-vendor: artifact_url UNPUBLISHED; pass --url or RELEASE_VENDOR_URL for a release" >&2
    exit 1
fi
echo "release-vendor: PASS artifact_url=$vendor_url"
