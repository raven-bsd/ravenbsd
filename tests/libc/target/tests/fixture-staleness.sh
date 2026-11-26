#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host regression: a fixture build must not silently reuse a stale libc sysroot.
#
# The test copies the real posix-poll build script into a temp tree, supplies an old
# sysroot and a newer libc source, and fakes cargo/clang/readelf. The fixed build must
# run `libc-pack` (or refuse an explicit stale `RAVENLIBC_SYSROOT`); before the fix it
# reused the old archive silently.

set -eu

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
repo=$(CDPATH= cd -- "$here/../../../.." && pwd)
tmp=${TMPDIR:-/tmp}/rl-fixture-stale-$$
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

tree=$tmp/tree
bin=$tmp/bin
mkdir -p "$tree/tests/fixtures/posix-poll" "$tree/tests/libc/target" \
    "$tree/libc/ravenlibc/crates" "$tree/libc/ravenlibc/abi" "$tree/libc/ravenlibc/crt" \
    "$tree/build/x86_64/sysroot/usr/lib" "$bin"

cp "$repo/tests/fixtures/posix-poll/build.sh" "$tree/tests/fixtures/posix-poll/build.sh"
if [ -f "$repo/tests/fixtures/libc-sysroot.sh" ]; then
    cp "$repo/tests/fixtures/libc-sysroot.sh" "$tree/tests/fixtures/libc-sysroot.sh"
fi
: > "$tree/tests/fixtures/posix-poll/main.c"
: > "$tree/tests/libc/target/x86_64.ld"
cp "$repo/tests/libc/target/shape-check.sh" "$tree/tests/libc/target/shape-check.sh"

sysroot=$tree/build/x86_64/sysroot
old=$tmp/old
touch -t 202001010000 "$old"
for file in libc.a crt1.o crti.o crtn.o; do
    cp -p "$old" "$sysroot/usr/lib/$file"
done
: > "$tree/libc/ravenlibc/crates/source.rs"
touch -t 202101010000 "$tree/libc/ravenlibc/crates/source.rs"

cat > "$bin/cargo" <<'EOF'
#!/bin/sh
printf '%s\n' "$*" >> "${RL_CARGO_LOG:?}"
case " $* " in
    *" --verify "*) [ -f "${RL_FRESH:?}" ] && exit 0 || exit 1 ;;
    *" libc-pack "*)
        : > "$RL_FRESH"
        touch -t 202501010000 "$RL_SYSROOT/libc.a" "$RL_SYSROOT/crt1.o" \
            "$RL_SYSROOT/crti.o" "$RL_SYSROOT/crtn.o"
        exit 0
        ;;
esac
exit 0
EOF
cat > "$bin/clang" <<'EOF'
#!/bin/sh
output=
previous=
for argument in "$@"; do
    if [ "$previous" = "-o" ]; then
        output=$argument
    fi
    previous=$argument
done
if [ -n "$output" ]; then
    dd if=/dev/zero of="$output" bs=4096 count=2 2>/dev/null
fi
exit 0
EOF
cat > "$bin/ld.lld" <<'EOF'
#!/bin/sh
exit 0
EOF
cat > "$bin/llvm-readelf" <<'EOF'
#!/bin/sh
cat <<'HEADERS'
Elf file type is EXEC (Executable file)
Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000200000 0x0000000000200000 0x001000 0x001000 R E 0x1000
  LOAD           0x001000 0x0000000000201000 0x0000000000201000 0x001000 0x001000 RW  0x1000
HEADERS
EOF
chmod +x "$bin/cargo" "$bin/clang" "$bin/ld.lld" "$bin/llvm-readelf"

log=$tmp/cargo.log
fresh=$tmp/fresh
passed=0
failed=0
ok() { passed=$((passed + 1)); }
bad() { failed=$((failed + 1)); printf 'fixture-staleness: FAIL %s\n' "$1" >&2; }

run_fixture() {
    PATH="$bin:$PATH" CC=clang RL_CARGO_LOG="$log" RL_FRESH="$fresh" \
        RL_SYSROOT="$sysroot/usr/lib" \
        sh "$tree/tests/fixtures/posix-poll/build.sh" x86_64
}
packed() {
    [ -f "$log" ] && grep -v -- '--verify' "$log" | grep -q 'libc-pack'
}

rm -f "$fresh"
: > "$log"
if run_fixture > "$tmp/stale.out" 2>&1; then ok; else bad "the stale fixture build exited nonzero"; fi
if packed; then ok; else bad "the stale fixture build silently reused the sysroot"; fi

: > "$log"
if run_fixture > "$tmp/fresh.out" 2>&1; then ok; else bad "the fresh fixture build exited nonzero"; fi
if packed; then bad "the fresh fixture build packed again"; else ok; fi

rm -f "$fresh"
: > "$log"
if (RAVENLIBC_SYSROOT="$sysroot" run_fixture) > "$tmp/explicit.out" 2>&1; then
    bad "an explicit stale sysroot was accepted"
else
    ok
fi
if packed; then bad "an explicit stale sysroot was packed behind the caller"; else ok; fi

printf 'fixture-staleness: %s passed, %s failed\n' "$passed" "$failed"
if [ "$failed" -ne 0 ]; then
    exit 1
fi
