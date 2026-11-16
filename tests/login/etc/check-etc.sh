#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# REL1c evidence: the release account files exist with the documented modes and the shadow
# record is a SHA-512-crypt hash with no plaintext password.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

check_mode() {
    path=$1
    expected=$2
    actual=$(stat -c '%a' "$path")
    if [ "$actual" != "$expected" ]; then
        printf 'REL1c: %s mode %s, expected %s\n' "$path" "$actual" "$expected" >&2
        exit 1
    fi
    printf 'REL1c: %s mode %s ok\n' "$path" "$actual"
}

check_mode etc/passwd 644
check_mode etc/group 644
check_mode etc/shadow 600
check_mode etc/profile 644
check_mode etc/motd 644

grep -Eq '^root:x:0:0:root:/root:/bin/brush$' etc/passwd
grep -Eq '^root:x:0:root$' etc/group
grep -Eq '^root:\$6\$' etc/shadow
if grep -Eq '^root:raven(:|$)' etc/shadow; then
    printf 'REL1c: plaintext root password in shadow\n' >&2
    exit 1
fi
printf 'REL1c: shadow hash ok\n'
printf 'REL1c: ok\n'
