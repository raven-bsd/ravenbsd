#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host dry-run tests for tests/libc/target/build.sh: a fake clang records its argv and a fake
# sysroot supplies the files build.sh checks, so the exact compile and link vectors of C73 4.4
# are asserted without a real ravenlibc sysroot or toolchain.

set -eu

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
build=$here/../build.sh
tmp=${TMPDIR:-/tmp}/rl-build-dryrun-$$
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

mkdir -p "$tmp/bin" "$tmp/sysroot/usr/lib" "$tmp/sysroot/lib" "$tmp/sysroot/libexec"
cat > "$tmp/bin/fake-clang" <<'EOF'
#!/bin/sh
printf '%s\n' "$*" >> "${RL_FAKE_LOG:?}"
output=
previous=
for argument in "$@"; do
    if [ "$previous" = "-o" ]; then
        output=$argument
    fi
    previous=$argument
done
if [ -n "$output" ]; then
    : > "$output"
fi
exit 0
EOF
cat > "$tmp/bin/ld.lld" <<'EOF'
#!/bin/sh
exit 0
EOF
cat > "$tmp/bin/fake-readelf" <<'EOF'
#!/bin/sh
cat <<'HEADERS'
Elf file type is EXEC (Executable file)
Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000200000 0x0000000000200000 0x001000 0x001000 R E 0x1000
  LOAD           0x001000 0x0000000000201000 0x0000000000201000 0x001000 0x001000 RW  0x1000
HEADERS
EOF
chmod +x "$tmp/bin/fake-clang" "$tmp/bin/ld.lld" "$tmp/bin/fake-readelf"

touch "$tmp/sysroot/usr/lib/crt1.o" "$tmp/sysroot/usr/lib/crti.o" \
    "$tmp/sysroot/usr/lib/crtn.o" "$tmp/sysroot/usr/lib/libc.a" \
    "$tmp/sysroot/usr/lib/Scrt1.o" "$tmp/sysroot/usr/lib/libc.so" \
    "$tmp/sysroot/lib/libc.so.1" "$tmp/sysroot/libexec/ld-elf.so.1"

passed=0
failed=0
ok() { passed=$((passed + 1)); }
bad() { failed=$((failed + 1)); printf 'build-dryrun: FAIL %s\n' "$1" >&2; }
contains() {
    if printf '%s\n' "$1" | grep -Fq -- "$2"; then ok; else bad "missing $2"; fi
}

log=$tmp/static.log
: > "$log"
RL_FAKE_LOG="$log" RL_FORCE=1 CC="$tmp/bin/fake-clang" PATH="$tmp/bin:$PATH" \
    RAVENLIBC_SYSROOT="$tmp/sysroot" READELF="$tmp/bin/fake-readelf" \
    sh "$build" hello x86_64
static=$(cat "$log")
contains "$static" '--target=x86_64-unknown-ravenbsd'
contains "$static" '--sysroot='
contains "$static" '-std=gnu11'
contains "$static" '-Werror=implicit-function-declaration'
contains "$static" '-D__ravenbsd__=1'
contains "$static" '-D__unix__=1'
contains "$static" '-mno-sse'
contains "$static" '-mno-mmx'
contains "$static" '-msoft-float'
contains "$static" 'crt1.o'
contains "$static" 'crti.o'
contains "$static" '-lc'
contains "$static" 'crtn.o'
contains "$static" '-static'
contains "$static" '-Wl,-z,max-page-size=4096'
contains "$static" '-Wl,-z,common-page-size=4096'
contains "$static" '-Wl,--error-limit=0'

log=$tmp/dynamic.log
: > "$log"
RL_FAKE_LOG="$log" RL_FORCE=1 CC="$tmp/bin/fake-clang" PATH="$tmp/bin:$PATH" \
    RAVENLIBC_SYSROOT="$tmp/sysroot" READELF="$tmp/bin/fake-readelf" \
    sh "$build" dynamic x86_64
dynamic=$(cat "$log")
contains "$dynamic" 'Scrt1.o'
contains "$dynamic" '-pie'
contains "$dynamic" '-Wl,-dynamic-linker,/libexec/ld-elf.so.1'
contains "$dynamic" '-shared'
contains "$dynamic" '-fPIC'
contains "$dynamic" '-Wl,-soname,libplug.so'

printf 'build-dryrun: %s passed, %s failed\n' "$passed" "$failed"
if [ "$failed" -ne 0 ]; then
    exit 1
fi
