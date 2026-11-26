#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Structural check for the U2 expected account files. The Rust tests do the byte-for-byte
# comparison; this script keeps the fixtures honest when they are edited by hand.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

sh tests/fixtures/user-management/check.sh

awk -F: '
	NF != 7 { printf "useradd-carol.passwd: line %d: %d fields\n", NR, NF; bad=1 }
	$1 == "carol" && ($3 != 1002 || $4 != 1002) { bad=1 }
	END { exit bad }
' tests/user/useradd-carol.passwd

awk -F: '
	NF != 4 { printf "useradd-carol.group: line %d: %d fields\n", NR, NF; bad=1 }
	$1 == "carol" && $3 != 1002 { bad=1 }
	END { exit bad }
' tests/user/useradd-carol.group

grep -q '^carol:!:0:0:99999:7:-1:-1:0$' tests/user/useradd-carol.shadow
grep -q '^alice:\$6\$testsalt\$.*:19001:0:99999:7:0:0:0$' tests/user/passwd-alice.shadow
grep -q '^staff:x:1002:$' tests/user/groupadd-staff.group

printf 'U2 user fixtures: ok\n'
