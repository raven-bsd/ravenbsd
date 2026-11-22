#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# served-copies.sh - refresh and check the served copies in the management tree.
#
# The product tree is the measurement source; the served copies are the only
# copies release tools may read.  A check never rewrites a served file.  A
# refresh writes only $RAVEN_MGMT_ROOT/architecture/product-data and the
# SERVED.tsv manifest.
#
# usage: sh scripts/served-copies.sh --check|--refresh [--root DIR] [--mgmt DIR]
#
# --check   fail when a served copy differs from its source or manifest; print
#           the exact refresh command.  A stale measurement stamp is a warning,
#           because only its generator can produce a new measurement.
# --refresh render the copies and rewrite SERVED.tsv; measured reports are
#           recorded in place and are never fabricated.

set -eu

usage() {
    cat <<'EOF'
usage: sh scripts/served-copies.sh --check|--refresh [--root DIR] [--mgmt DIR]

  --check     compare each served copy with its source and SERVED.tsv
  --refresh   render the copies and rewrite architecture/product-data/SERVED.tsv
  --root DIR  product checkout (default: the parent of this script)
  --mgmt DIR  management checkout (default: RAVEN_MGMT_ROOT or ../ravenbsd-prjmgmt)
EOF
}

mode=
root=
mgmt=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --check|--refresh)
            mode=$1
            ;;
        --root)
            shift
            [ "$#" -gt 0 ] || { usage >&2; exit 2; }
            root=$1
            ;;
        --mgmt)
            shift
            [ "$#" -gt 0 ] || { usage >&2; exit 2; }
            mgmt=$1
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
    shift
done

[ -n "$mode" ] || { usage >&2; exit 2; }

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ -z "$root" ]; then
    root=$(CDPATH= cd -- "$script_dir/.." && pwd)
fi
if [ ! -d "$root" ]; then
    printf 'served-copies: product root %s is not a directory\n' "$root" >&2
    exit 2
fi
if [ -z "$mgmt" ]; then
    if [ -n "${RAVEN_MGMT_ROOT:-}" ]; then
        mgmt=$RAVEN_MGMT_ROOT
    else
        mgmt=$root/../ravenbsd-prjmgmt
    fi
fi
if [ ! -d "$mgmt" ]; then
    printf 'served-copies: RAVEN_MGMT_ROOT: management root %s is not a directory; set RAVEN_MGMT_ROOT to the ravenbsd-prjmgmt checkout\n' "$mgmt" >&2
    exit 2
fi

data=$mgmt/architecture/product-data
manifest=$data/SERVED.tsv
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

entries=$tmp/entries
cat > "$entries" <<'EOF'
measured|mgmt|architecture/product-data/kernel-gate.md|architecture/product-data/kernel-gate.md|sh scripts/kernel-gate.sh
measured|mgmt|architecture/product-data/posix-conformance.md|architecture/product-data/posix-conformance.md|cargo run -q -p posix-conform -- gate --root . --profile posix --arch x86_64
render-port|product|tests/release/port-register/register.tsv|architecture/product-data/port-register.md|
render-abi|product|tests/abi/abi-v1.txt|architecture/product-data/syscall-abi.md|
render-ssh|mgmt|architecture/release-path.tsv|architecture/product-data/ssh-gate.tsv|
EOF

entry_path() {
    case "$1" in
        product) printf '%s/%s\n' "$root" "$2" ;;
        mgmt) printf '%s/%s\n' "$mgmt" "$2" ;;
        *) return 1 ;;
    esac
}

repo_path() {
    case "$1" in
        product) printf '%s\n' "$root" ;;
        mgmt) printf '%s\n' "$mgmt" ;;
        *) return 1 ;;
    esac
}

sha256() {
    sha256sum "$1" | awk '{ print $1 }'
}

head_for() {
    repo=$1
    relative=$2
    if [ -n "${SERVED_COPIES_TEST_HEAD:-}" ]; then
        printf '%s\n' "$SERVED_COPIES_TEST_HEAD"
        return 0
    fi
    commit=$(git -C "$repo" rev-parse HEAD 2>/dev/null || true)
    if [ -z "$commit" ]; then
        printf 'unknown\n'
        return 0
    fi
    if [ -n "$(git -C "$repo" status --porcelain -- "$relative" 2>/dev/null || true)" ]; then
        printf '%s-dirty\n' "$commit"
    else
        printf '%s\n' "$commit"
    fi
}

measured_commit() {
    report=$1
    case "$report" in
        *kernel-gate.md)
            commit=$(grep -oE '`[0-9a-f]{40}`' "$report" 2>/dev/null | head -n 1 | tr -d '`' || true)
            if [ -z "$commit" ]; then
                commit=$(grep -oE '`[0-9a-f]{12}`' "$report" 2>/dev/null | head -n 1 | tr -d '`' || true)
            fi
            ;;
        *posix-conformance.md)
            commit=$(sed -n 's/^- Commit: `\([^`]*\)`.*$/\1/p' "$report" 2>/dev/null | head -n 1 || true)
            ;;
        *)
            commit=unknown
            ;;
    esac
    if [ -z "$commit" ]; then
        commit=unknown
    fi
    printf '%s\n' "$commit"
}

render_port() {
    source=$1
    served=$2
    out=$3
    counts=$tmp/port-counts
    none=$tmp/port-none
    base=$tmp/port-base.md

    awk -F'\t' '
        NR == 1 { next }
        NF < 3 { next }
        {
            area = $2
            status = $3
            total[area]++
            if (status == "COVERED" || status == "INDIRECT" || status == "NONE") {
                count[area, status]++
            }
        }
        END {
            n = split("kernel libc crates userland", areas, " ")
            print "| Area | Donor files | COVERED | INDIRECT | NONE |"
            print "|---|---:|---:|---:|---:|"
            for (i = 1; i <= n; i++) {
                a = areas[i]
                printf "| `%s/` | %d | %d | %d | %d |\n",
                    a, total[a] + 0, count[a, "COVERED"] + 0,
                    count[a, "INDIRECT"] + 0, count[a, "NONE"] + 0
                tt += total[a] + 0
                tc += count[a, "COVERED"] + 0
                ti += count[a, "INDIRECT"] + 0
                tn += count[a, "NONE"] + 0
            }
            printf "| **Total** | **%d** | **%d** | **%d** | **%d** |\n", tt, tc, ti, tn
        }
    ' "$source" > "$counts"

    awk -F'\t' '
        NR > 1 && $3 == "NONE" {
            printf "| `%s` | %s | %s |\n", $1, $6, $7
        }
    ' "$source" > "$none"

    if [ -f "$served" ]; then
        port_doc=$served
    else
        cat > "$base" <<'EOF'
<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
<!-- GENERATED by scripts/served-copies.sh from tests/release/port-register/register.tsv; do not edit. -->

# 0.1.0 donor port test register

Source: `tests/release/port-register/register.tsv`. Regenerate it with
`python3 tests/release/port-register/run.py --write`; the reducer gate is
`python3 tests/release/port-register/run.py --check`.

This is a static file-to-test mapping, not line or branch coverage. A COVERED row
means a test file or fixture names the donor file's path, basename or module stem;
INDIRECT means only the file's crate has tests; NONE means neither.

## Counts

## NONE rows
EOF
        port_doc=$base
    fi

    awk -v counts="$counts" -v none="$none" '
        function emit_counts(   line) {
            print "## Counts"
            print ""
            while ((getline line < counts) > 0) {
                print line
            }
            close(counts)
            print ""
        }
        function emit_none(   line, any) {
            print "## NONE rows"
            print ""
            print "Every NONE row needs a test or an explicit exception; this ticket does not write those tests."
            print ""
            print "| File | Owner | Proposed ticket |"
            print "|---|---|---|"
            any = 0
            while ((getline line < none) > 0) {
                print line
                any = 1
            }
            close(none)
            if (!any) {
                print "| - | - | - |"
            }
            print ""
        }
        /^## Counts$/ { emit_counts(); skip = 1; next }
        /^## NONE rows$/ { emit_none(); skip = 1; next }
        skip && /^## / { skip = 0 }
        skip { next }
        { print }
    ' "$port_doc" > "$out"
}

render_abi() {
    source=$1
    served=$2
    out=$3
    base=$tmp/abi-base.md

    if [ -f "$served" ]; then
        abi_doc=$served
    else
        cat > "$base" <<'EOF'
<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Native syscall ABI (v1 freeze)

## Syscalls

| Number | Name | Args | Return | Label |
|---|---|---|---|---|

## Wire structs
EOF
        abi_doc=$base
    fi

    awk -v src="$source" '
        FNR == NR {
            if ($0 ~ /^\| `0x[0-9A-Fa-f]+` \| `[[:alnum:]_]+` \|/) {
                n = split($0, fields, "|")
                name = fields[3]
                label = fields[6]
                gsub(/[ `]/, "", name)
                gsub(/^[[:space:]]+|[[:space:]]+$/, "", label)
                labels[name] = label
            }
            next
        }
        /^\| Number \| Name \| Args \| Return \| Label \|$/ {
            print
            getline
            print
            while ((getline line < src) > 0) {
                n = split(line, fields, " ")
                if (fields[1] != "SYSCALL") {
                    continue
                }
                number = fields[2]
                name = fields[3]
                argc = fields[4]
                ret = fields[5]
                sub(/^argc=/, "", argc)
                sub(/^ret=/, "", ret)
                prefix = substr(number, 1, 2)
                digits = substr(number, 3)
                number = prefix toupper(digits)
                label = labels[name]
                if (label == "") {
                    label = "Evolving"
                }
                printf "| `%s` | `%s` | %s | `%s` | %s |\n", number, name, argc, ret, label
            }
            close(src)
            in_table = 1
            next
        }
        in_table && /^$/ { in_table = 0; print; next }
        in_table { next }
        { print }
    ' "$abi_doc" "$abi_doc" > "$out"
}

render_ssh() {
    source=$1
    out=$2
    {
        printf '# SPDX-License-Identifier: BSD-3-Clause\n'
        printf '# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n'
        printf '# GENERATED by scripts/served-copies.sh from architecture/release-path.tsv; do not edit.\n'
        printf '# area\tstep\tcapability\tstate\tdetail\towner\n'
        awk -F'\t' '$1 == "ssh" { print }' "$source"
    } > "$out"
}

render_entry() {
    entry_mode=$1
    source=$2
    served=$3
    out=$4
    case "$entry_mode" in
        measured)
            [ -f "$source" ] || return 1
            cp "$source" "$out"
            ;;
        render-port)
            render_port "$source" "$served" "$out"
            ;;
        render-abi)
            render_abi "$source" "$served" "$out"
            ;;
        render-ssh)
            render_ssh "$source" "$out"
            ;;
        *)
            return 1
            ;;
    esac
}

entry_count=$(awk 'NF { n++ } END { print n + 0 }' "$entries")
stale=0
warn=0
failed=0

if [ "$mode" = --check ]; then
    if [ ! -f "$manifest" ]; then
        printf 'served-copies: MANIFEST missing %s; run: sh scripts/served-copies.sh --refresh\n' "$manifest" >&2
        failed=1
    fi
    index=0
    while IFS='|' read -r entry_mode entry_repo source_rel served_rel generator; do
        [ -n "$entry_mode" ] || continue
        index=$((index + 1))
        source=$(entry_path "$entry_repo" "$source_rel")
        served=$(entry_path mgmt "$served_rel")
        out=$tmp/render.$index

        if ! render_entry "$entry_mode" "$source" "$served" "$out"; then
            printf 'served-copies: MISSING %s (source %s); refresh with: %s\n' \
                "$served_rel" "$source_rel" "$generator" >&2
            stale=$((stale + 1))
            failed=1
            continue
        fi
        if [ ! -f "$served" ] || ! cmp -s "$out" "$served"; then
            printf 'served-copies: STALE %s (source %s); run: sh scripts/served-copies.sh --refresh\n' \
                "$served_rel" "$source_rel" >&2
            stale=$((stale + 1))
            failed=1
        fi
        [ -f "$served" ] || continue

        expected_commit=$(measured_commit "$served")
        head=$(head_for "$root" .)
        if [ "$entry_mode" = measured ] && [ "$expected_commit" != "$head" ]; then
            printf 'served-copies: WARN %s measurement commit %s != HEAD %s; refresh with: %s\n' \
                "$served_rel" "$expected_commit" "$head" "$generator" >&2
            warn=$((warn + 1))
        fi

        if [ -f "$manifest" ]; then
            row=$(awk -F'\t' -v s="$served_rel" '!/^#/ && $1 == s { print; found = 1 } END { if (!found) exit 1 }' "$manifest" 2>/dev/null || true)
            if [ -z "$row" ]; then
                printf 'served-copies: MANIFEST missing row %s; run: sh scripts/served-copies.sh --refresh\n' "$served_rel" >&2
                stale=$((stale + 1))
                failed=1
                continue
            fi
            IFS="$(printf '\t')" read -r m_served m_source m_repo m_source_sha m_served_sha m_commit m_time m_entry_mode <<EOF
$row
EOF
            if [ "$m_served" != "$served_rel" ] || \
               [ "$m_source" != "$source_rel" ] || \
               [ "$m_repo" != "$entry_repo" ] || \
               [ "$m_entry_mode" != "$entry_mode" ]; then
                printf 'served-copies: MANIFEST stale %s; run: sh scripts/served-copies.sh --refresh\n' "$served_rel" >&2
                stale=$((stale + 1))
                failed=1
                continue
            fi
            source_sha=$(sha256 "$source")
            served_sha=$(sha256 "$served")
            if [ "$m_source_sha" != "$source_sha" ] || [ "$m_served_sha" != "$served_sha" ]; then
                printf 'served-copies: MANIFEST stale %s; run: sh scripts/served-copies.sh --refresh\n' "$served_rel" >&2
                stale=$((stale + 1))
                failed=1
                continue
            fi
            if [ "$entry_mode" = measured ]; then
                row_commit=$expected_commit
            else
                row_commit=$(head_for "$(repo_path "$entry_repo")" "$source_rel")
            fi
            if [ "$m_commit" != "$row_commit" ]; then
                printf 'served-copies: MANIFEST stale %s (source commit %s != %s); run: sh scripts/served-copies.sh --refresh\n' \
                    "$served_rel" "$m_commit" "$row_commit" >&2
                stale=$((stale + 1))
                failed=1
            fi
        fi
    done < "$entries"

    if [ -f "$manifest" ]; then
        rows=$(awk '!/^#/ && NF { n++ } END { print n + 0 }' "$manifest")
        if [ "$rows" -ne "$entry_count" ]; then
            printf 'served-copies: MANIFEST has %s rows, expected %s; run: sh scripts/served-copies.sh --refresh\n' \
                "$rows" "$entry_count" >&2
            stale=$((stale + 1))
            failed=1
        fi
    fi

    if [ "$failed" -ne 0 ]; then
        printf 'served-copies: FAIL (%s stale, %s measurement warning%s)\n' \
            "$stale" "$warn" "$( [ "$warn" -eq 1 ] && printf '' || printf 's' )"
        exit 1
    fi
    printf 'served-copies: PASS (%s served copies current, %s measurement warning%s)\n' \
        "$entry_count" "$warn" "$( [ "$warn" -eq 1 ] && printf '' || printf 's' )"
    exit 0
fi

mkdir -p "$data"
index=0
while IFS='|' read -r entry_mode entry_repo source_rel served_rel generator; do
    [ -n "$entry_mode" ] || continue
    index=$((index + 1))
    source=$(entry_path "$entry_repo" "$source_rel")
    served=$(entry_path mgmt "$served_rel")
    out=$tmp/render.$index
    if ! render_entry "$entry_mode" "$source" "$served" "$out"; then
        printf 'served-copies: MISSING %s (source %s); refresh with: %s\n' \
            "$served_rel" "$source_rel" "$generator" >&2
        exit 1
    fi
done < "$entries"

now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
new_manifest=$tmp/SERVED.tsv
{
    printf '# SPDX-License-Identifier: BSD-3-Clause\n'
    printf '# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n'
    printf '# GENERATED by scripts/served-copies.sh --refresh; do not edit.\n'
    printf '# served\tsource\trepo\tsource_sha256\tserved_sha256\tsource_commit\tgenerated_utc\tmode\n'
} > "$new_manifest"

index=0
while IFS='|' read -r entry_mode entry_repo source_rel served_rel generator; do
    [ -n "$entry_mode" ] || continue
    index=$((index + 1))
    source=$(entry_path "$entry_repo" "$source_rel")
    served=$(entry_path mgmt "$served_rel")
    out=$tmp/render.$index

    if [ "$entry_mode" = measured ]; then
        printf 'served-copies: recorded %s (measured in place)\n' "$served_rel"
    else
        cp "$out" "$served.tmp"
        mv "$served.tmp" "$served"
        printf 'served-copies: refreshed %s\n' "$served_rel"
    fi

    source_sha=$(sha256 "$source")
    served_sha=$(sha256 "$served")
    if [ "$entry_mode" = measured ]; then
        commit=$(measured_commit "$served")
        head=$(head_for "$root" .)
        if [ "$commit" != "$head" ]; then
            printf 'served-copies: WARN %s measurement commit %s != HEAD %s; refresh with: %s\n' \
                "$served_rel" "$commit" "$head" "$generator" >&2
        fi
    else
        commit=$(head_for "$(repo_path "$entry_repo")" "$source_rel")
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$served_rel" "$source_rel" "$entry_repo" "$source_sha" "$served_sha" \
        "$commit" "$now" "$entry_mode" >> "$new_manifest"
done < "$entries"

mv "$new_manifest" "$manifest"
printf 'served-copies: refreshed %s\n' "$manifest"
