#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Standard RavenBSD Make help renderer.
#
# Usage: sh make-help.sh "Title" scripts/help.txt
#
# Manifest format:
#   group:Name          start a coloured group
#   target|description  one aligned entry; long descriptions wrap at 78 columns
#   # ...               comment
#
# Colour is enabled only on a terminal. NO_COLOR, TERM=dumb, MAKE_HELP_COLOR=0 disable it;
# MAKE_HELP_COLOR=1 forces it for documentation/tests. MAKE_HELP_ASCII=1 forces ASCII rules.

set -eu

title=${1:-RavenBSD}
manifest=${2:-scripts/help.txt}
[ -r "$manifest" ] || {
    printf 'make-help: missing manifest %s\n' "$manifest" >&2
    exit 2
}

color=0
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ] && [ "${TERM:-}" != dumb ] &&
    [ "${MAKE_HELP_COLOR:-1}" != 0 ]; then
    color=1
fi
if [ "${MAKE_HELP_COLOR:-}" = 1 ]; then
    color=1
fi

ascii=${MAKE_HELP_ASCII:-0}
case ${TERM:-} in dumb) ascii=1 ;; esac
[ "$color" -eq 1 ] || ascii=1

if [ "$color" -eq 1 ]; then
    esc=$(printf '\033')
    c_title="${esc}[38;5;198m"
    c_rule="${esc}[38;5;51m"
    c_group="${esc}[38;5;51m"
    c_target="${esc}[38;5;154m"
    c_desc="${esc}[38;5;245m"
    c_reset="${esc}[0m"
else
    c_title=
    c_rule=
    c_group=
    c_target=
    c_desc=
    c_reset=
fi

printf '%s%s%s\n' "$c_title" "$title" "$c_reset"
if [ "$ascii" -eq 1 ]; then
    printf '%s%s%s\n' "$c_rule" '----------------------------------------------------------------' "$c_reset"
else
    printf '%s%s%s\n' "$c_rule" '────────────────────────────────────────────────────────────────' "$c_reset"
fi
printf '%sUsage: make <target> [VARIABLE=value]%s\n' "$c_desc" "$c_reset"

while IFS= read -r raw || [ -n "$raw" ]; do
    line=$(printf '%s' "$raw" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
    case $line in
        ''|'#'*) continue ;;
        group:*)
            group=${line#group:}
            printf '\n%s%s%s\n' "$c_group" "$group" "$c_reset"
            ;;
        *'|'*)
            target=${line%%|*}
            desc=${line#*|}
            # 22-column target field; wrap the description at 78 columns.
            width=22
            text_width=$((78 - 2 - width - 1))
            printf '%s\n' "$desc" | fold -s -w "$text_width" | {
                first=1
                while IFS= read -r chunk || [ -n "$chunk" ]; do
                    if [ "$first" -eq 1 ]; then
                        printf '  %s%-*s%s %s%s%s\n' \
                            "$c_target" "$width" "$target" "$c_reset" "$c_desc" "$chunk" "$c_reset"
                        first=0
                    else
                        printf '  %*s%s %s%s%s\n' \
                            "$width" '' "$c_reset" "$c_desc" "$chunk" "$c_reset"
                    fi
                done
            }
            ;;
        *)
            printf 'make-help: bad manifest line: %s\n' "$line" >&2
            exit 2
            ;;
    esac
done < "$manifest"

printf '\n%sUse NO_COLOR=1 or MAKE_HELP_COLOR=0 for plain output.%s\n' "$c_desc" "$c_reset"
