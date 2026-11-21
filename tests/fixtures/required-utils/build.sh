#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the P4a10 required-utility target fixture and pack build/fixtures/required-utils/initfs.img.
# RAVEN_QT_ARCH selects x86_64 (default) or aarch64; the qtest pre step sets it from --arch.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/required-utils
mkdir -p "$out/etc"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        rustflags="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld"
        ;;
    *)
        echo "required-utils fixture: unsupported architecture $arch" >&2
        exit 1
        ;;
esac
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build -p required-utils -p localedef -p lp -p newgrp -p raven-time -p times -p tput -p ulimit \
        -p raven-initfsd -p raven-ramfsd -p raven-vfsd --target "$target" --release

cat >"$out/etc/passwd" <<'EOF'
root:x:0:0:root:/root:/bin/sh
EOF

cat >"$out/etc/group" <<'EOF'
root:x:0:root
users:x:100:root
EOF

cat >"$out/etc/shadow" <<'EOF'
root:*:19000:0:99999:7:::
EOF

cat >"$out/etc/locale" <<'EOF'
LC_NUMERIC
decimal_point "."
thousands_sep ","
END LC_NUMERIC
LC_MESSAGES
yesexpr "^[yY]"
noexpr "^[nN]"
END LC_MESSAGES
EOF

cat >"$out/etc/charmap" <<'EOF'
<U002E> /x2e U+002E FULL STOP
<U002C> /x2c U+002C COMMA
EOF

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/required-utils-init:0755" \
    "/bin/time=$target_dir/$target/release/time:0755" \
    "/bin/times=$target_dir/$target/release/times:0755" \
    "/bin/tput=$target_dir/$target/release/tput:0755" \
    "/bin/ulimit=$target_dir/$target/release/ulimit:0755" \
    "/bin/localedef=$target_dir/$target/release/localedef:0755" \
    "/bin/lp=$target_dir/$target/release/lp:0755" \
    "/bin/newgrp=$target_dir/$target/release/newgrp:0755" \
    "/bin/true=$target_dir/$target/release/required-utils-true:0755" \
    "/bin/sh=$target_dir/$target/release/required-utils-shell:0755" \
    "/sbin/initfsd=$target_dir/$target/release/raven-initfsd:0755" \
    "/sbin/ramfsd=$target_dir/$target/release/raven-ramfsd:0755" \
    "/sbin/vfsd=$target_dir/$target/release/raven-vfsd:0755" \
    "/etc/passwd=$out/etc/passwd:0644" \
    "/etc/group=$out/etc/group:0644" \
    "/etc/shadow=$out/etc/shadow:0644" \
    "/etc/locale=$out/etc/locale:0644" \
    "/etc/charmap=$out/etc/charmap:0644"
