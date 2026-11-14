#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host tests for tests/libc/target/shape-check.sh: a good 4 KiB ELF and an RWX segment fixture
# are fed through the same parser the build uses.

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
cat > "$tmp/fake-readelf" <<'EOF'
#!/bin/sh
cat "${RL_SHAPE_FIXTURE:?}"
EOF
chmod +x "$tmp/fake-readelf"

touch "$tmp/elf"
passed=0
failed=0
ok() { passed=$((passed + 1)); }
bad() { failed=$((failed + 1)); printf 'shape-test: FAIL %s\n' "$1" >&2; }

out=$(READELF="$tmp/fake-readelf" RL_SHAPE_FIXTURE="$tmp/good.txt" sh "$shape" "$tmp/elf")
if [ "$out" = "shape: ok" ]; then ok; else bad "good fixture: $out"; fi

set +e
out=$(READELF="$tmp/fake-readelf" RL_SHAPE_FIXTURE="$tmp/rwx.txt" sh "$shape" "$tmp/elf")
status=$?
set -e
if [ "$status" -eq 1 ] && printf '%s\n' "$out" | grep -Fq 'writable and executable'; then
    ok
else
    bad "rwx fixture: status $status, $out"
fi

set +e
out=$(READELF="$tmp/fake-readelf" RL_SHAPE_FIXTURE="$tmp/bad-align.txt" sh "$shape" "$tmp/elf")
status=$?
set -e
if [ "$status" -eq 1 ] && printf '%s\n' "$out" | grep -Fq 'align'; then
    ok
else
    bad "align fixture: status $status, $out"
fi

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
