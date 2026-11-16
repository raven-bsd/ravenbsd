#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Validate the canonical user-management fixtures. These are parser/test vectors, not a
# production account database; no hash here is a release credential except the REL1b root vector.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
cd "$root"

fail() {
	printf 'user-management fixture: %s\n' "$*" >&2
	exit 1
}

awk -F: '
	NF != 7 { printf "passwd: %s: expected 7 fields, got %d\n", $1, NF; bad=1 }
	$1 !~ /^[a-z_][a-z0-9_-]*$/ { printf "passwd: bad name %s\n", $1; bad=1 }
	$2 != "x" { printf "passwd: %s: password field is not x\n", $1; bad=1 }
	seen[$1]++ { printf "passwd: duplicate name %s\n", $1; bad=1 }
	uid[$3]++ { printf "passwd: duplicate uid %s\n", $3; bad=1 }
	END { exit bad }
' passwd || fail "passwd fields"

awk -F: '
	NF != 4 { printf "group: %s: expected 4 fields, got %d\n", $1, NF; bad=1 }
	seen[$1]++ { printf "group: duplicate name %s\n", $1; bad=1 }
	gid[$3]++ { printf "group: duplicate gid %s\n", $3; bad=1 }
	END { exit bad }
' group || fail "group fields"

awk -F: '
	NF != 9 { printf "shadow: %s: expected 9 fields, got %d\n", $1, NF; bad=1 }
	seen[$1]++ { printf "shadow: duplicate name %s\n", $1; bad=1 }
	END { exit bad }
' shadow || fail "shadow fields"

# Every passwd primary gid exists, and every group member is a passwd name.
awk -F: '
	NR == FNR { gid[$3] = 1; next }
	!($4 in gid) { printf "passwd: %s: unknown primary gid %s\n", $1, $4; bad=1 }
	END { exit bad }
' group passwd || fail "passwd primary groups"

awk -F: '
	NR == FNR { name[$1] = 1; next }
	{
		n = split($4, members, ",")
		for (i = 1; i <= n; i++) {
			if (members[i] != "" && !(members[i] in name)) {
				printf "group: %s: unknown member %s\n", $1, members[i]
				bad = 1
			}
		}
	}
	END { exit bad }
' passwd group || fail "group members"

grep -q '^root:\$6\$' shadow || fail "root shadow is not SHA-512-crypt"
grep -q '^alice:\$6\$' shadow || fail "alice shadow is not SHA-512-crypt"
grep -q '^bob:!' shadow || fail "bob is not locked"
[ "$(awk -F: '$1 == "root" { print $3 }' passwd)" = 0 ] || fail "root uid is not 0"
[ "$(awk -F: '$1 == "root" { print $4 }' passwd)" = 0 ] || fail "root gid is not 0"

printf 'user-management fixtures: ok\n'
