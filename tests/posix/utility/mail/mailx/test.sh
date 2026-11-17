#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P47 mailx behaviour: send/list/read/delete on a Maildir, body from a file.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../../../.." && pwd)
target=$root/build/posix-utility/mail-host

if [ -z "${MAILX_BIN:-}" ]; then
    (
        cd "$root"
        CARGO_TARGET_DIR=$target cargo build --quiet --locked -p mailx
    )
    MAILX_BIN=$target/debug/mailx
fi
if [ ! -x "$MAILX_BIN" ]; then
    printf 'mailx test: missing host binary\n' >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/Maildir/new" "$tmp/Maildir/cur" "$tmp/Maildir/tmp"
export MAIL=$tmp/Maildir
cd "$tmp"

printf 'maildir body\n' > "$tmp/body.txt"
"$MAILX_BIN" -s 'Maildir subject' -b "$tmp/body.txt" alice
"$MAILX_BIN" > list
grep -q '^1 Maildir subject ' list
"$MAILX_BIN" -r 1 > read
grep -q '^Subject: Maildir subject$' read
grep -q '^maildir body$' read
if [ "$(find "$tmp/Maildir/new" -type f | wc -l)" -ne 1 ]; then
    printf 'mailx test: expected one new Maildir file\n' >&2
    exit 1
fi
printf 'ok: Maildir send/list/read from file\n'

"$MAILX_BIN" -d 1 > delete
grep -q 'deleted' delete
if [ "$(find "$tmp/Maildir/new" "$tmp/Maildir/cur" -type f | wc -l)" -ne 0 ]; then
    printf 'mailx test: message file remains after delete\n' >&2
    exit 1
fi
if "$MAILX_BIN" -e; then
    printf 'mailx test: -e should fail on an empty Maildir\n' >&2
    exit 1
fi
printf 'ok: Maildir delete and -e\n'

if "$MAILX_BIN" -F 2> error; then
    printf 'mailx test: -F should be rejected\n' >&2
    exit 1
fi
grep -q 'unsupported option' error
printf 'ok: unsupported forward is a usage error\n'
