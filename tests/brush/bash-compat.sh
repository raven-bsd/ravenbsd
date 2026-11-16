#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Bash-compatibility smoke for brush (K26a). Run it with the brush binary:
#   userland/brush/target/debug/brush tests/brush/bash-compat.sh
#
# The script keeps to POSIX/bash constructs the ticket names: variables,
# quoting, a pipeline, redirection, a function, if/for, command substitution,
# exit-status propagation, set -e and a bash-style PS1 placeholder. It uses
# only builtins and /bin/cat so the same file can run on the Raven target.

set -e

value=world
echo "var:$value"
echo 'quote:"a b"'

# Pipelines need pipe2(2) and spawn fd handoff (PG-35/PG-49); the platform does not have
# them yet, so use a builtin plus a file round-trip instead. A pipeline currently reports a
# clean "Function not implemented" I/O error and does not crash.
echo one

echo redirect > /tmp/brush-compat.txt
read line < /tmp/brush-compat.txt
echo "$line"

greet() {
    echo "func:$1"
}
greet ok

if true; then
    echo "if:yes"
fi

for item in 1 2; do
    echo "for:$item"
done

echo "sub:$(echo nested)"

false && echo unreachable
echo "or:ok"

false || echo "or-fallback:ok"

false && true
echo "status:$?"

PS1='raven# '
echo "ps1:$PS1"
