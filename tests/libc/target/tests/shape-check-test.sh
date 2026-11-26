#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host tests for tests/libc/target/shape-check.sh: good 4 KiB ELFs, an RWX segment, a segment
# past EOF and the TLS program-header/section invariants are fed through the same parser the
# build uses.

set -eu

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
shape=$here/../shape-check.sh
tmp=${TMPDIR:-/tmp}/rl-shape-test-$$
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

mkdir -p "$tmp"
cat > "$tmp/good.txt" <<'EOF'
Elf file type is EXEC
Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000200000 0x0000000000200000 0x001000 0x001000 R E 0x1000
  LOAD           0x001000 0x0000000000201000 0x0000000000201000 0x001000 0x001000 RW  0x1000
EOF
cat > "$tmp/rwx.txt" <<'EOF'
Elf file type is EXEC
Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000200000 0x0000000000200000 0x001000 0x001000 RWE 0x1000
EOF
cat > "$tmp/bad-align.txt" <<'EOF'
Elf file type is EXEC
Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000200000 0x0000000000200000 0x001000 0x001000 R E 0x200000
EOF
cat > "$tmp/bad-tail.txt" <<'EOF'
Elf file type is EXEC
Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000200000 0x0000000000200000 0x001234 0x002000 R E 0x1000
EOF
cat > "$tmp/tls-good.txt" <<'EOF'
Elf file type is EXEC
Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000200000 0x0000000000200000 0x001000 0x001000 R E 0x1000
  LOAD           0x001000 0x0000000000201000 0x0000000000201000 0x001000 0x001000 RW  0x1000
  TLS            0x001000 0x0000000000201000 0x0000000000201000 0x000010 0x000020 R   0x4
EOF
cat > "$tmp/tls-sections.txt" <<'EOF'
Section Headers:
  [Nr] Name              Type            Address          Off    Size   ES Flg Lk Inf Al
  [ 1] .text             PROGBITS        0000000000200000 000000 001000 00  AX  0   0  4
  [ 2] .tdata            PROGBITS        0000000000201000 001000 000010 00 WAT  0   0  4
  [ 3] .tbss             NOBITS          0000000000201010 001010 000010 00 WAT  0   0  4
EOF
cat > "$tmp/tls-missing-phdr.txt" <<'EOF'
Elf file type is EXEC
Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000200000 0x0000000000200000 0x001000 0x001000 R E 0x1000
  LOAD           0x001000 0x0000000000201000 0x0000000000201000 0x001000 0x001000 RW  0x1000
EOF
cat > "$tmp/tls-short-mem.txt" <<'EOF'
Elf file type is EXEC
Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000200000 0x0000000000200000 0x001000 0x001000 R E 0x1000
  LOAD           0x001000 0x0000000000201000 0x0000000000201000 0x001000 0x001000 RW  0x1000
  TLS            0x001000 0x0000000000201000 0x0000000000201000 0x000010 0x000010 R   0x4
EOF
cat > "$tmp/tls-bad-align.txt" <<'EOF'
Elf file type is EXEC
Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000200000 0x0000000000200000 0x001000 0x001000 R E 0x1000
  LOAD           0x001000 0x0000000000201000 0x0000000000201000 0x001000 0x001000 RW  0x1000
  TLS            0x001000 0x0000000000201001 0x0000000000201001 0x000010 0x000020 R   0x3
EOF
cat > "$tmp/fake-readelf" <<'EOF'
#!/bin/sh
case "$*" in
    *-SW*) cat "${RL_SHAPE_SECTIONS:-/dev/null}" ;;
    *) cat "${RL_SHAPE_FIXTURE:?}" ;;
esac
EOF
chmod +x "$tmp/fake-readelf"

passed=0
failed=0
ok() { passed=$((passed + 1)); }
bad() { failed=$((failed + 1)); printf 'shape-test: FAIL %s\n' "$1" >&2; }

# The ELF must be at least as large as its file-backed segments; `pages` sets the fixture size.
make_elf() {
    dd if=/dev/zero of="$tmp/elf" bs=4096 count="$1" 2>/dev/null
}

expect_ok() {
    label=$1
    pages=$2
    program=$3
    sections=$4
    make_elf "$pages"
    out=$(READELF="$tmp/fake-readelf" RL_SHAPE_FIXTURE="$program" RL_SHAPE_SECTIONS="$sections" \
        sh "$shape" "$tmp/elf")
    if [ "$out" = "shape: ok" ]; then ok; else bad "$label fixture: $out"; fi
}

expect_fail() {
    label=$1
    pages=$2
    program=$3
    sections=$4
    pattern=$5
    make_elf "$pages"
    set +e
    out=$(READELF="$tmp/fake-readelf" RL_SHAPE_FIXTURE="$program" RL_SHAPE_SECTIONS="$sections" \
        sh "$shape" "$tmp/elf")
    status=$?
    set -e
    if [ "$status" -eq 1 ] && printf '%s\n' "$out" | grep -Fq "$pattern"; then
        ok
    else
        bad "$label fixture: status $status, $out"
    fi
}

expect_ok "good" 64 "$tmp/good.txt" ""
expect_fail "rwx" 64 "$tmp/rwx.txt" "" 'writable and executable'
expect_fail "align" 64 "$tmp/bad-align.txt" "" 'align'
expect_fail "tail" 1 "$tmp/bad-tail.txt" "" 'past EOF'
expect_ok "tls good" 64 "$tmp/tls-good.txt" "$tmp/tls-sections.txt"
expect_fail "tls missing phdr" 64 "$tmp/tls-missing-phdr.txt" "$tmp/tls-sections.txt" 'without PT_TLS'
expect_fail "tls short mem" 64 "$tmp/tls-short-mem.txt" "$tmp/tls-sections.txt" 'tbss'
expect_fail "tls bad align" 64 "$tmp/tls-bad-align.txt" "$tmp/tls-sections.txt" 'PT_TLS align'

make_elf 64
set +e
out=$(READELF="$tmp/fake-readelf" RL_SHAPE_FIXTURE="$tmp/good.txt" sh "$shape" "$tmp/missing")
status=$?
set -e
if [ "$status" -eq 1 ] && printf '%s\n' "$out" | grep -Fq 'missing'; then
    ok
else
    bad "missing fixture: status $status, $out"
fi

printf 'shape-test: %s passed, %s failed\n' "$passed" "$failed"
if [ "$failed" -ne 0 ]; then
    exit 1
fi
