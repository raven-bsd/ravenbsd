#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Deterministic CPU load generator for the K376 TLB stress gate.
#
# `start N PIDFILE` starts N fixed-work yes(1) workers (one work unit each, no random input).
# `stop PIDFILE` kills exactly those workers and waits for them.

set -eu

action=${1:-}
case "$action" in
    start)
        workers=${2:-0}
        pidfile=${3:?start needs a pidfile}
        case "$workers" in ''|*[!0-9]*) echo "load-generator: bad worker count" >&2; exit 2 ;; esac
        : > "$pidfile"
        index=0
        while [ "$index" -lt "$workers" ]; do
            yes >/dev/null &
            printf '%s\n' "$!" >> "$pidfile"
            index=$((index + 1))
        done
        echo "load-generator: started workers=$workers pidfile=$pidfile"
        ;;
    stop)
        pidfile=${2:?stop needs a pidfile}
        [ -f "$pidfile" ] || exit 0
        while IFS= read -r pid; do
            [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
        done < "$pidfile"
        wait 2>/dev/null || true
        rm -f "$pidfile"
        echo "load-generator: stopped pidfile=$pidfile"
        ;;
    *)
        echo "usage: $0 start N PIDFILE | stop PIDFILE" >&2
        exit 2
        ;;
esac
