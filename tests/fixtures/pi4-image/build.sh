#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the Pi 4 base/shell/server image variants twice, verify the FAT boot
# manifest and the GPT ROOT partition, and prove that two builds of the same
# variant are byte-identical while the three variants differ.
#
# This fixture needs the read-only Raspberry Pi firmware checkout. The image
# builder records the repository head in the kernel build id, so the two builds
# must run against the same worktree; the check proves determinism, not a fixed
# hash value.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/pi4-image
variants="pi4-base pi4-shell pi4-server"
results=$out/variants.tsv

mkdir -p "$out"
rm -f "$results"

if [ -z "${RAVEN_RPI_FIRMWARE:-}" ]; then
    for candidate in \
        "$root/../ravenbsd-resources/rpi-firmware/boot" \
        "$root/../../ravenbsd-resources/rpi-firmware/boot" \
        "$root/../../../ravenbsd-resources/rpi-firmware/boot" \
        "${HOME:-/nonexistent}/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot" \
        "${HOME:-/nonexistent}/ravenbsd-resources/rpi-firmware/boot"
    do
        if [ -d "$candidate" ]; then
            RAVEN_RPI_FIRMWARE=$candidate
            export RAVEN_RPI_FIRMWARE
            break
        fi
    done
fi
[ -n "${RAVEN_RPI_FIRMWARE:-}" ] && [ -d "$RAVEN_RPI_FIRMWARE" ] || {
    echo "pi4-image: set RAVEN_RPI_FIRMWARE to the firmware boot directory" >&2
    exit 1
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        echo "pi4-image: sha256sum or shasum is required" >&2
        exit 2
    fi
}

printf 'variant\timage_sha256\tkernel8_sha256\troot_partition\n' >"$results"

for variant in $variants; do
    first=$out/$variant.a.img
    second=$out/$variant.b.img
    cargo xtask image --arch aarch64 --platform pi4 --profile "$variant" --output "$first" \
        >"$out/$variant.build-a.log" 2>&1
    cargo xtask image --arch aarch64 --platform pi4 --profile "$variant" --output "$second" \
        >"$out/$variant.build-b.log" 2>&1

    first_sha=$(sha256_file "$first")
    second_sha=$(sha256_file "$second")
    if [ "$first_sha" != "$second_sha" ]; then
        echo "pi4-image: $variant is not deterministic: $first_sha != $second_sha" >&2
        exit 1
    fi
    rm -f "$second"

    scripts/pi4-serial.sh --check -i "$first" >"$out/$variant.manifest.log"
    grep -q 'pi4-manifest: PASSED' "$out/$variant.manifest.log" || {
        echo "pi4-image: $variant manifest check failed" >&2
        exit 1
    }

    # The GPT ROOT partition is the J5 UFS-root hook. Parse the primary table without host
    # partition tools so the check is reproducible on a bare CI host.
    root_info=$(python3 - "$first" <<'PY'
import struct
import sys

path = sys.argv[1]
with open(path, "rb") as image:
    image.seek(512)
    header = image.read(92)
    if header[:8] != b"EFI PART":
        raise SystemExit("pi4-image: missing GPT header")
    entries_lba = struct.unpack_from("<Q", header, 72)[0]
    count = struct.unpack_from("<I", header, 80)[0]
    size = struct.unpack_from("<I", header, 84)[0]
    image.seek(entries_lba * 512)
    entries = image.read(count * size)

for index in range(count):
    entry = entries[index * size:(index + 1) * size]
    if entry[:16] == b"\0" * 16:
        continue
    name = entry[56:128].decode("utf-16le", "replace").split("\0", 1)[0]
    if name == "ROOT":
        first = struct.unpack_from("<Q", entry, 32)[0]
        last = struct.unpack_from("<Q", entry, 40)[0]
        print(f"{index + 1} {first} {last}")
        break
else:
    raise SystemExit("pi4-image: no ROOT partition")
PY
)
    set -- $root_info
    [ "$#" -eq 3 ] || {
        echo "pi4-image: cannot parse the ROOT partition" >&2
        exit 1
    }
    [ "$1" -eq 2 ] || {
        echo "pi4-image: ROOT is GPT partition $1, expected partition 2 for the J5 path" >&2
        exit 1
    }

    mtype -i "$first@@1M" "::/kernel8.img" >"$out/$variant.kernel8.img" 2>/dev/null || {
        echo "pi4-image: cannot read $variant kernel8.img" >&2
        exit 1
    }
    kernel_sha=$(sha256_file "$out/$variant.kernel8.img")
    printf '%s\t%s\t%s\t%s..%s\n' "$variant" "$first_sha" "$kernel_sha" "$2" "$3" >>"$results"
    echo "pi4-image: $variant image=$first_sha kernel8=$kernel_sha root=$2..$3"
done

unique=$(cut -f2 "$results" | tail -n +2 | sort -u | wc -l | tr -d ' ')
[ "$unique" -eq 3 ] || {
    echo "pi4-image: expected three distinct variant images, got $unique" >&2
    exit 1
}

echo "pi4-image: variants deterministic and distinct"
cat "$results"
