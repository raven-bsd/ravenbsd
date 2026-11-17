#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P47 mail behaviour: send/list/read/delete on an mbox store, body from stdin and file.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../../../.." && pwd)
target=$root/build/posix-utility/mail-host

if [ -z "${MAIL_BIN:-}" ]; then
    (
        cd "$root"
        CARGO_TARGET_DIR=$target cargo build --quiet --locked -p mail
    )
    MAIL_BIN=$target/debug/mail
fi
if [ ! -x "$MAIL_BIN" ]; then
    printf 'mail test: missing host binary\n' >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export MAIL=$tmp/mbox
cd "$tmp"

printf 'body line\nsecond line\n' | "$MAIL_BIN" -s 'Test subject' alice
"$MAIL_BIN" > list
grep -q '^1 Test subject ' list
"$MAIL_BIN" -r 1 > read
grep -q '^Subject: Test subject$' read
grep -q '^body line$' read
printf 'ok: mbox send/list/read from stdin\n'

printf 'file body\n' > "$tmp/body.txt"
printf 'ignored stdin\n' | "$MAIL_BIN" -s 'File subject' -b "$tmp/body.txt" bob
"$MAIL_BIN" > list
grep -q '^2 File subject ' list
"$MAIL_BIN" -r 2 > read
grep -q '^file body$' read
printf 'ok: mbox body from file\n'

"$MAIL_BIN" -d 1 > delete
grep -q 'deleted' delete
"$MAIL_BIN" > list
if grep -q 'Test subject' list; then
    printf 'mail test: deleted message is still listed\n' >&2
    exit 1
fi
"$MAIL_BIN" -d 1 > delete
grep -q 'deleted' delete
"$MAIL_BIN" > list
if grep -q 'File subject' list; then
    printf 'mail test: second deleted message is still listed\n' >&2
    exit 1
fi
if "$MAIL_BIN" -e; then
    printf 'mail test: -e should fail after the last message is deleted\n' >&2
    exit 1
fi
printf 'ok: mbox delete and -e\n'

if "$MAIL_BIN" -R 2> error; then
    printf 'mail test: -R should be rejected\n' >&2
    exit 1
fi
grep -q 'unsupported option' error
printf 'ok: unsupported reply is a usage error\n'
