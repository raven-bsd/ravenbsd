#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX behaviour evidence for the bounded first-party `localedef`.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../../.." && pwd)
cd "$root"
cargo nextest run -p localedef

# The gate runs this script directly, without the runner's UTIL export.
if [ -z "${UTIL:-}" ]; then
    cargo build -q -p localedef
    UTIL="$root/target/debug/localedef"
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/locale" <<'EOF'
LC_NUMERIC
decimal_point "."
thousands_sep ","
END LC_NUMERIC
LC_MESSAGES
yesexpr "^[yY]"
noexpr "^[nN]"
END LC_MESSAGES
EOF

cat > "$tmp/charmap" <<'EOF'
<U002E> /x2e U+002E FULL STOP
<U002C> /x2c U+002C COMMA
EOF

"$UTIL" -i "$tmp/locale" -f "$tmp/charmap" -o "$tmp/catalog" testlocale
magic=$(od -An -tx1 -N4 "$tmp/catalog" | tr -d ' \n')
[ "$magic" = "ff88ff89" ] || { echo "bad catalog magic: $magic"; exit 1; }
printf 'ok: localedef emits the runtime catalog image\n'

cat > "$tmp/bad" <<'EOF'
LC_FOO
x "y"
END LC_FOO
EOF

if "$UTIL" -i "$tmp/bad" -f "$tmp/charmap" -o "$tmp/bad.cat" >/dev/null 2>&1; then
    echo "unsupported source was accepted"
    exit 1
fi
[ ! -e "$tmp/bad.cat" ] || { echo "partial catalog was written"; exit 1; }
printf 'ok: localedef rejects unsupported syntax without writing\n'

if "$UTIL" -i "$tmp/locale" >/dev/null 2>&1; then
    echo "missing -f was accepted"
    exit 1
fi
printf 'ok: localedef requires the charmap option\n'
