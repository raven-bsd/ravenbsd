#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Evidence index (K544): walk every ticket report, parse the Evidence (ASPICE/ASIL) block, and
# write one newline-stable TSV register with the report hash and every referenced artifact.
#
# usage: sh scripts/evidence-index.sh [--root DIR] [--check | --refresh | --self-test]
#
#   (default)     index the management root and write tickets/evidence/INDEX.tsv;
#                 exits non-zero on a DONE report without an Evidence block, a missing
#                 artifact, or a COMPLETE claim with a PARTIAL row.
#   --refresh     the same as the default, but regenerate every artifact hash from scratch
#                 (ignore the persistent cache); writes only into $RAVEN_MGMT_ROOT.
#   --check       the same audit without writing; prints the counts and the gap list and
#                 exits 0 so a backlog can be measured before it is fixed.
#   --self-test   prove the three failure paths with temporary reports under /tmp and
#                 check that a clean report indexes deterministically.
#
# The management root is $RAVEN_MGMT_ROOT, default ../ravenbsd-prjmgmt relative to the
# product repository root. A nested minion clone (<workspace>/minions/<lane>/ravenbsd)
# falls back to ../../../ravenbsd-prjmgmt when the sibling path does not exist. Artifacts
# are resolved under tickets/evidence/ (including artifacts/ for copied /tmp logs) and
# then the product tree for large files.

set -eu
LC_ALL=C
export LC_ALL

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
if [ -n "${RAVEN_MGMT_ROOT:-}" ]; then
    MGMT=$RAVEN_MGMT_ROOT
else
    MGMT=
    if [ -d "$ROOT/../ravenbsd-prjmgmt/tickets" ]; then
        MGMT=$ROOT/../ravenbsd-prjmgmt
    else
        # Nested minion clone (<workspace>/minions/<lane>/ravenbsd): resolve the logical
        # parent with the shell so `..` is not applied to the clone's symlink target.
        parent=
        if parent=$(CDPATH= cd -- "$ROOT/../../../" 2>/dev/null && pwd -L); then
            if [ -n "$parent" ] && [ -d "$parent/ravenbsd-prjmgmt/tickets" ]; then
                MGMT=$parent/ravenbsd-prjmgmt
            fi
        fi
    fi
    [ -n "$MGMT" ] || MGMT=$ROOT/../ravenbsd-prjmgmt
fi
MODE=collect
while [ "$#" -gt 0 ]; do
    case "$1" in
        --root)
            [ "$#" -ge 2 ] || { echo "evidence-index: --root needs a directory" >&2; exit 2; }
            MGMT=$2
            shift 2
            ;;
        --check)
            MODE=check
            shift
            ;;
        --self-test)
            MODE=self-test
            shift
            ;;
        --refresh)
            MODE=refresh
            shift
            ;;
        -h | --help)
            sed -n '3,20p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "evidence-index: unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

if [ ! -d "$MGMT/tickets" ]; then
    echo "evidence-index: no tickets directory under $MGMT" >&2
    exit 2
fi
MGMT=$(CDPATH= cd -- "$MGMT" && pwd)
EVIDENCE=$MGMT/tickets/evidence

WORK=$(mktemp -d "${TMPDIR:-/tmp}/evidence-index.XXXXXX")
trap 'rm -rf "$WORK"' EXIT HUP INT TERM

sha256_of() {
    sha256sum "$1" | cut -d' ' -f1
}

size_of() {
    wc -c <"$1" | tr -d ' '
}

# Print "OK<TAB>path" when the referenced path exists as written, "MOVED<TAB>path" when it exists
# under tickets/evidence/ after a relocation, or nothing when it does not exist.
#
# K558: K551 named reports that still point at the legacy product tests tree whose "evidence"
# component moved to tickets/evidence/ in management. Such a path is rewritten to
# tickets/evidence/<rest> and reported as MOVED. The lookup also tries the direct path, the ticket
# directory, the flat evidence directories, then a unique basename match (also MOVED).
resolve_path() {
    ticket=$1
    token=$2
    base=$(basename -- "$token")
    case $token in
        tickets/evidence/*)
            if [ -f "$MGMT/$token" ]; then
                printf 'OK\t%s' "$MGMT/$token"
                return 0
            fi
            ;;
        tests/*)
            rest=${token#tests/}
            if [ "${rest%%/*}" = "evidence" ]; then
                legacy=${rest#evidence/}
                if [ -f "$ROOT/$token" ]; then
                    printf 'OK\t%s' "$ROOT/$token"
                    return 0
                fi
                if [ -f "$MGMT/tickets/evidence/$legacy" ]; then
                    printf 'MOVED\t%s' "$MGMT/tickets/evidence/$legacy"
                    return 0
                fi
            fi
            for candidate in "$MGMT/$token" "$ROOT/$token"; do
                if [ -f "$candidate" ]; then
                    printf 'OK\t%s' "$candidate"
                    return 0
                fi
            done
            ;;
        /*) ;;
        *)
            for candidate in "$MGMT/$token" "$ROOT/$token"; do
                if [ -f "$candidate" ]; then
                    printf 'OK\t%s' "$candidate"
                    return 0
                fi
            done
            ;;
    esac
    for candidate in "$EVIDENCE/artifacts/$ticket/$base" "$EVIDENCE/artifacts/$base" "$EVIDENCE/$base"; do
        if [ -f "$candidate" ]; then
            printf 'MOVED\t%s' "$candidate"
            return 0
        fi
    done
    matches=$(find "$EVIDENCE" -type f -name "$base" 2>/dev/null || true)
    count=$(printf '%s\n' "$matches" | grep -c . || true)
    if [ "$count" -eq 1 ]; then
        printf 'MOVED\t%s' "$matches"
    fi
    return 0
}

# Characters that cannot appear in an artifact token we index.
path_like() {
    printf '%s' "$1" | awk '
        length($0) == 0 { exit 1 }
        /\/$/ { exit 1 }
        /[ \t<>|$(),;*[]/ { exit 1 }
        /:[0-9]+(-[0-9]+)?$/ { exit 1 }
        /^(~|\/proc|\/sys|\/dev\/)/ { exit 1 }
        /^\./ { exit 1 }
        /(^|\/)(tickets|tests)\/evidence\// { exit 0 }
        {
            base = $0
            sub(/^.*\//, "", base)
            if (base ~ /^\./) { exit 1 }
            if (base ~ /\.(log|json|txt|csv|tsv|xml|pcap|pcapng|img|bin|iso|png|jpg|jpeg|svg|pdf|sha256|tar|gz|tgz|zip|zst|a|so|elf|map|out|serial|list)$/) { exit 0 }
        }
        { exit 1 }
    '
}

# audit_root MODE
# MODE=collect|refresh writes INDEX.tsv and enforces; refresh rehashes every artifact and
# rebuilds the cache. MODE=check only reports.
audit_root() {
    mode=$1
    use_cache=1
    if [ "$mode" = "refresh" ]; then
        use_cache=0
    fi
    rows=$WORK/rows
    failures=$WORK/failures
    warnings=$WORK/warnings
    : >"$rows"
    : >"$failures"
    : >"$warnings"
    : >"$WORK/hash-cache"

    # Persistent artifact-hash cache: path, size, mtime, sha256. A changed artifact is rehashed;
    # an unchanged one (including the 335 MB images) is reused across runs.
    cache=$EVIDENCE/.index-cache
    cache_lookup() {
        [ -f "$cache" ] || return 0
        awk -F'\t' -v path="$1" -v size="$2" -v mtime="$3" \
            '$1 == path && $2 == size && $3 == mtime { print $4; exit }' "$cache"
    }
    cache_store() {
        printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" >>"$cache"
    }
    if [ "$mode" = "collect" ] || [ "$mode" = "refresh" ]; then
        mkdir -p "$EVIDENCE"
    fi
    if [ "$mode" = "refresh" ]; then
        : >"$cache"
    fi

    reports=0
    done_reports=0
    indexed=0
    reports_list=$WORK/reports
    find "$MGMT/tickets" -type f -path '*/reports/*.md' | sort >"$reports_list"
    reports=$(wc -l <"$reports_list" | tr -d ' ')

    while IFS= read -r report; do
        [ -n "$report" ] || continue
        ticket=$(basename "$report" .md)
        lane=$(basename "$(dirname "$(dirname "$report")")")
        result=$(awk '
            /^Result:[[:space:]]*/ {
                value = $0
                sub(/^Result:[[:space:]]*/, "", value)
                split(value, word, /[ \t]/)
                print word[1]
                exit
            }
        ' "$report")
        [ -n "$result" ] || result=UNKNOWN
        if [ "$result" = "DONE" ]; then
            done_reports=$((done_reports + 1))
        else
            printf 'WARN %s: result %s\n' "$ticket" "$result" >>"$warnings"
        fi

        block=$WORK/block
        awk '
            /^## Evidence \(ASPICE\/ASIL\)[[:space:]]*$/ { inside = 1; next }
            inside && /^## / { exit }
            inside { print }
        ' "$report" >"$block"
        if [ -s "$block" ]; then
            has_evidence=1
        else
            has_evidence=0
        fi

        trace_rows=0
        partial_rows=0
        if [ "$has_evidence" -eq 1 ]; then
            counts=$(awk -v ticket="$ticket" '
                /^\|/ {
                    fields = split($0, cell, "|")
                    first = cell[2]
                    gsub(/^[ \t]+|[ \t]+$/, "", first)
                    if (first != ticket "-R" && first !~ ("^" ticket "-R[0-9]+$")) {
                        next
                    }
                    rows++
                    status = ""
                    for (slot = fields; slot >= 2; slot--) {
                        value = cell[slot]
                        gsub(/^[ \t]+|[ \t]+$/, "", value)
                        if (value != "") {
                            status = value
                            break
                        }
                    }
                    if (status ~ /PARTIAL/) {
                        partial++
                    }
                }
                END { print rows + 0, partial + 0 }
            ' "$block")
            trace_rows=$(printf '%s' "$counts" | cut -d' ' -f1)
            partial_rows=$(printf '%s' "$counts" | cut -d' ' -f2)
        fi

        claims_complete=0
        if head -n 15 "$report" | grep -Eiq '^(evidence|completeness):[[:space:]]*\**[[:space:]]*complete'; then
            claims_complete=1
        fi

        report_sha=$(sha256_of "$report")
        report_bytes=$(size_of "$report")

        artifacts=$WORK/artifacts
        if [ "$has_evidence" -eq 1 ]; then
            grep -o '`[^`]*`' "$block" | tr -d '`' | sort -u >"$artifacts.raw" || true
            : >"$artifacts"
            while IFS= read -r token; do
                [ -n "$token" ] || continue
                if path_like "$token"; then
                    printf '%s\n' "$token" >>"$artifacts"
                fi
            done <"$artifacts.raw"
            sort -u "$artifacts" -o "$artifacts"
        else
            : >"$artifacts"
        fi

        resolved_list=$WORK/resolved
        : >"$resolved_list"
        missing_here=0
        while IFS= read -r token; do
            [ -n "$token" ] || continue
            resolution=$(resolve_path "$ticket" "$token")
            if [ -n "$resolution" ]; then
                state=$(printf '%s' "$resolution" | cut -f1)
                resolved=$(printf '%s' "$resolution" | cut -f2)
                printf '%s\t%s\t%s\n' "$token" "$resolved" "$state" >>"$resolved_list"
            else
                missing_here=$((missing_here + 1))
                printf '%s\t-\tMISSING\n' "$token" >>"$resolved_list"
                printf 'FAIL %s: artifact missing: %s\n' "$ticket" "$token" >>"$failures"
            fi
        done <"$artifacts"

        completeness=COMPLETE
        if [ "$result" != "DONE" ]; then
            completeness=WARN
        elif [ "$has_evidence" -ne 1 ]; then
            completeness=MISSING
            printf 'FAIL %s: DONE report has no Evidence (ASPICE/ASIL) block\n' "$ticket" >>"$failures"
        elif [ "$partial_rows" -ne 0 ] || [ "$missing_here" -ne 0 ]; then
            completeness=PARTIAL
        fi

        evidence_state=missing
        if [ "$has_evidence" -eq 1 ]; then
            evidence_state=present
        fi

        emitted=0
        while IFS='	' read -r token resolved state; do
            [ -n "$token" ] || continue
            if [ "$state" = "OK" ] || [ "$state" = "MOVED" ]; then
                bytes=$(size_of "$resolved")
                mtime=$(stat -c %Y "$resolved")
                sum=
                if [ "$use_cache" -eq 1 ]; then
                    sum=$(cache_lookup "$resolved" "$bytes" "$mtime")
                fi
                if [ -z "$sum" ]; then
                    meta=$(awk -F'\t' -v path="$resolved" '$1 == path { print $2 "\t" $3; exit }' "$WORK/hash-cache")
                    if [ -n "$meta" ]; then
                        sum=$(printf '%s\n' "$meta" | cut -f1)
                    else
                        sum=$(sha256_of "$resolved")
                        printf '%s\t%s\t%s\n' "$resolved" "$sum" "$bytes" >>"$WORK/hash-cache"
                        if [ "$mode" = "collect" ] || [ "$mode" = "refresh" ]; then
                            cache_store "$resolved" "$bytes" "$mtime" "$sum"
                        fi
                    fi
                fi
                printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
                    "$ticket" "$lane" "$result" "$evidence_state" "$completeness" \
                    "$trace_rows" "$partial_rows" "$report" "$report_sha" "$report_bytes" \
                    "$token" "$sum" "$bytes" "$state" >>"$rows"
            else
                printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
                    "$ticket" "$lane" "$result" "$evidence_state" "$completeness" \
                    "$trace_rows" "$partial_rows" "$report" "$report_sha" "$report_bytes" \
                    "$token" - - MISSING >>"$rows"
            fi
            emitted=$((emitted + 1))
        done <"$resolved_list"

        if [ "$emitted" -eq 0 ]; then
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
                "$ticket" "$lane" "$result" "$evidence_state" "$completeness" \
                "$trace_rows" "$partial_rows" "$report" "$report_sha" "$report_bytes" \
                - - - NONE >>"$rows"
        fi

        if [ "$claims_complete" -eq 1 ] && [ "$partial_rows" -ne 0 ]; then
            awk -v ticket="$ticket" '
                /^\|/ {
                    fields = split($0, cell, "|")
                    first = cell[2]
                    gsub(/^[ \t]+|[ \t]+$/, "", first)
                    if (first !~ ("^" ticket "-R[0-9]+$")) {
                        next
                    }
                    status = ""
                    for (slot = fields; slot >= 2; slot--) {
                        value = cell[slot]
                        gsub(/^[ \t]+|[ \t]+$/, "", value)
                        if (value != "") {
                            status = value
                            break
                        }
                    }
                    if (status ~ /PARTIAL/) {
                        print "FAIL " ticket ": report claims COMPLETE but row " first " is PARTIAL"
                    }
                }
            ' "$block" >>"$failures"
        fi

        indexed=$((indexed + 1))
    done <"$reports_list"

    sort -u "$rows" -o "$rows"
    failures_sorted=$WORK/failures.sorted
    warnings_sorted=$WORK/warnings.sorted
    sort -u "$failures" >"$failures_sorted"
    sort -u "$warnings" >"$warnings_sorted"

    complete=$(awk -F'\t' '$5 == "COMPLETE" { print $1 }' "$rows" | sort -u | wc -l | tr -d ' ')
    done_missing=$(awk -F'\t' '$5 == "MISSING" { print $1 }' "$rows" | sort -u | wc -l | tr -d ' ')
    missing_artifacts=$(awk -F'\t' '$NF == "MISSING" && $11 != "-" { count++ } END { print count + 0 }' "$rows")
    moved_artifacts=$(awk -F'\t' '$NF == "MOVED" && $11 != "-" { count++ } END { print count + 0 }' "$rows")
    partial_claims=$(grep -c 'claims COMPLETE but row' "$failures_sorted" || true)
    warning_count=$(wc -l <"$warnings_sorted" | tr -d ' ')
    failure_count=$(wc -l <"$failures_sorted" | tr -d ' ')

    printf 'evidence-index: tickets=%s reports=%s done=%s complete=%s done-missing-evidence=%s missing-artifacts=%s moved-artifacts=%s partial-claims=%s warnings=%s failures=%s\n' \
        "$indexed" "$reports" "$done_reports" "$complete" "$done_missing" \
        "$missing_artifacts" "$moved_artifacts" "$partial_claims" "$warning_count" "$failure_count"

    if [ "$mode" = "collect" ] || [ "$mode" = "refresh" ]; then
        mkdir -p "$EVIDENCE"
        index=$EVIDENCE/INDEX.tsv
        out=$WORK/INDEX.tsv
        printf 'ticket\tlane\tresult\tevidence\tcompleteness\ttrace_rows\tpartial_rows\treport\treport_sha256\treport_bytes\tartifact\tartifact_sha256\tartifact_bytes\tartifact_state\n' >"$out"
        cat "$rows" >>"$out"
        cp "$out" "$index"
        printf 'evidence-index: index=%s sha256=%s\n' "$index" "$(sha256_of "$index")"
        if [ -f "$cache" ]; then
            sort -t '	' -k1,1 -k3,3n "$cache" \
                | awk -F'\t' '{ line[$1] = $0 } END { for (key in line) print line[key] }' \
                | sort >"$cache.new"
            mv "$cache.new" "$cache"
        fi
    else
        printf 'evidence-index: index not written (--check)\n'
    fi
    while IFS= read -r line; do
        [ -n "$line" ] && printf 'evidence-index: %s\n' "$line"
    done <"$failures_sorted"
    while IFS= read -r line; do
        [ -n "$line" ] && printf 'evidence-index: %s\n' "$line"
    done <"$warnings_sorted"

    if { [ "$mode" = "collect" ] || [ "$mode" = "refresh" ]; } && [ "$failure_count" -ne 0 ]; then
        return 1
    fi
    return 0
}

self_test() {
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/evidence-index-self.XXXXXX")
    trap 'rm -rf "$WORK" "$tmp"' EXIT HUP INT TERM
    status=0

    make_root() {
        root=$1
        mkdir -p "$root/tickets/steve/reports" "$root/tickets/evidence/artifacts"
        printf 'ok\n' >"$root/tickets/evidence/artifacts/good.log"
    }

    write_good() {
        cat >"$tmp/good/tickets/steve/reports/K900.md" <<'EOF'
# K900 report
Directive rev: 1
Result: DONE

## Evidence (ASPICE/ASIL)

| ID | Requirement | Test | Result |
|---|---|---|---|
| K900-R01 | index the artifact | `good.log` | PASS |
EOF
    }

    write_no_evidence() {
        cat >"$tmp/noev/tickets/steve/reports/K901.md" <<'EOF'
# K901 report
Directive rev: 1
Result: DONE
EOF
    }

    write_missing_artifact() {
        cat >"$tmp/noart/tickets/steve/reports/K902.md" <<'EOF'
# K902 report
Directive rev: 1
Result: DONE

## Evidence (ASPICE/ASIL)

| ID | Requirement | Test | Result |
|---|---|---|---|
| K902-R01 | missing artifact | `gone.log` | PASS |
EOF
    }

    write_complete_partial() {
        cat >"$tmp/partial/tickets/steve/reports/K903.md" <<'EOF'
# K903 report
Directive rev: 1
Result: DONE
Evidence: COMPLETE

## Evidence (ASPICE/ASIL)

| ID | Requirement | Test | Result |
|---|---|---|---|
| K903-R01 | blocked sub-row | `good.log` | PARTIAL |
EOF
    }

    write_warning() {
        cat >"$tmp/warn/tickets/steve/reports/K904.md" <<'EOF'
# K904 report
Directive rev: 1
Result: BLOCKED
EOF
    }

    write_moved() {
        legacy="tests/$(printf 'evidence')/group-a/dup.log"
        cat >"$tmp/moved/tickets/steve/reports/K905.md" <<EOF
# K905 report
Directive rev: 1
Result: DONE

## Evidence (ASPICE/ASIL)

| ID | Requirement | Test | Result |
|---|---|---|---|
| K905-R01 | old product evidence path | \`$legacy\` | PASS |
EOF
        mkdir -p "$tmp/moved/tickets/evidence/group-a" "$tmp/moved/tickets/evidence/group-b"
        printf 'moved\n' >"$tmp/moved/tickets/evidence/group-a/dup.log"
        printf 'decoy\n' >"$tmp/moved/tickets/evidence/group-b/dup.log"
    }

    expect_fail() {
        name=$1
        root=$2
        pattern=$3
        output=$WORK/self-$name
        if RAVEN_MGMT_ROOT=$root sh "$0" >"$output" 2>&1; then
            printf 'FAIL self-test %s: expected non-zero exit\n' "$name"
            status=1
            return 0
        fi
        if ! grep -q "$pattern" "$output"; then
            printf 'FAIL self-test %s: missing message: %s\n' "$name" "$pattern"
            status=1
            return 0
        fi
        printf 'ok self-test %s: %s\n' "$name" "$pattern"
    }

    for name in good noev noart partial warn moved; do
        make_root "$tmp/$name"
    done
    write_good
    write_no_evidence
    write_missing_artifact
    write_complete_partial
    write_warning
    write_moved

    output=$WORK/self-good
    if ! RAVEN_MGMT_ROOT=$tmp/good sh "$0" >"$output" 2>&1; then
        printf 'FAIL self-test good: expected zero exit\n'
        status=1
    elif ! grep -q 'tickets=1 reports=1 done=1 complete=1' "$output"; then
        printf 'FAIL self-test good: unexpected counts\n'
        status=1
    elif ! grep -q "$(sha256sum "$tmp/good/tickets/evidence/artifacts/good.log" | cut -d' ' -f1)" \
        "$tmp/good/tickets/evidence/INDEX.tsv"; then
        printf 'FAIL self-test good: artifact hash missing from INDEX.tsv\n'
        status=1
    else
        first=$(sha256sum "$tmp/good/tickets/evidence/INDEX.tsv" | cut -d' ' -f1)
        RAVEN_MGMT_ROOT=$tmp/good sh "$0" >"$output" 2>&1
        second=$(sha256sum "$tmp/good/tickets/evidence/INDEX.tsv" | cut -d' ' -f1)
        if [ "$first" != "$second" ]; then
            printf 'FAIL self-test good: INDEX.tsv is not deterministic\n'
            status=1
        else
            printf 'ok self-test good: counts, artifact hash, stable index\n'
        fi
    fi

    expect_fail noev "$tmp/noev" 'DONE report has no Evidence (ASPICE/ASIL) block'
    expect_fail noart "$tmp/noart" 'artifact missing: gone.log'
    expect_fail partial "$tmp/partial" 'report claims COMPLETE but row K903-R01 is PARTIAL'

    output=$WORK/self-warn
    if ! RAVEN_MGMT_ROOT=$tmp/warn sh "$0" >"$output" 2>&1; then
        printf 'FAIL self-test warn: non-DONE must only warn\n'
        status=1
    elif ! grep -q 'WARN K904: result BLOCKED' "$output"; then
        printf 'FAIL self-test warn: missing warning\n'
        status=1
    else
        printf 'ok self-test warn: non-DONE warns and exits zero\n'
    fi

    output=$WORK/self-moved
    if ! RAVEN_MGMT_ROOT=$tmp/moved sh "$0" >"$output" 2>&1; then
        printf 'FAIL self-test moved: a relocated artifact must resolve\n'
        status=1
    elif ! grep -q 'moved-artifacts=1' "$output"; then
        printf 'FAIL self-test moved: expected one MOVED artifact\n'
        status=1
    elif ! grep -q 'MOVED' "$tmp/moved/tickets/evidence/INDEX.tsv"; then
        printf 'FAIL self-test moved: INDEX.tsv does not say MOVED\n'
        status=1
    else
        printf 'ok self-test moved: old product evidence path resolves as MOVED\n'
    fi

    return "$status"
}

case $MODE in
    self-test)
        self_test
        ;;
    collect)
        audit_root collect
        ;;
    refresh)
        audit_root refresh
        ;;
    check)
        audit_root check
        ;;
esac
