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

echo one | /bin/cat

echo redirect > /tmp/brush-compat.txt
/bin/cat /tmp/brush-compat.txt

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
