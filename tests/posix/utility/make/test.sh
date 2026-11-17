#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host acceptance cases for the first-party make utility.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo build -q -p make
make_bin="$root/target/debug/make"
[ -x "$make_bin" ] || {
    echo "make test: $make_bin is missing" >&2
    exit 1
}

work="$root/build/posix/utility/make"
rm -rf "$work"
mkdir -p "$work"
trap 'rm -rf "$work"' EXIT

fail() {
    echo "make test: FAIL $*" >&2
    exit 1
}

case_variables() {
    dir="$work/variables"
    mkdir -p "$dir"
    cat >"$dir/Makefile" <<'EOF'
GREETING = hello
NAME := world
all:
	@printf '%s %s\n' '$(GREETING)' '$(NAME)'
EOF
    out=$(cd "$dir" && "$make_bin")
    [ "$out" = "hello world" ] || fail "variables: $out"
}

case_pattern() {
    dir="$work/pattern"
    mkdir -p "$dir"
    : >"$dir/out.c"
    cat >"$dir/Makefile" <<'EOF'
all: out.o
%.o: %.c
	@printf 'pattern %s -> %s\n' '$<' '$@'
EOF
    out=$(cd "$dir" && "$make_bin")
    [ "$out" = "pattern out.c -> out.o" ] || fail "pattern: $out"
}

case_suffix() {
    dir="$work/suffix"
    mkdir -p "$dir"
    : >"$dir/suf.c"
    cat >"$dir/Makefile" <<'EOF'
all: suf.o
.c.o:
	@printf 'suffix %s -> %s\n' '$<' '$@'
EOF
    out=$(cd "$dir" && "$make_bin")
    [ "$out" = "suffix suf.c -> suf.o" ] || fail "suffix: $out"
}

case_phony() {
    dir="$work/phony"
    mkdir -p "$dir"
    cat >"$dir/Makefile" <<'EOF'
.PHONY: clean
clean:
	@printf 'clean\n'
EOF
    out=$(cd "$dir" && "$make_bin" clean)
    [ "$out" = "clean" ] || fail "phony: $out"
}

case_include() {
    dir="$work/include"
    mkdir -p "$dir"
    printf 'VALUE = included\n' >"$dir/inc.mk"
    cat >"$dir/Makefile" <<'EOF'
include inc.mk
all:
	@printf '%s\n' '$(VALUE)'
EOF
    out=$(cd "$dir" && "$make_bin")
    [ "$out" = "included" ] || fail "include: $out"
}

case_automatic() {
    dir="$work/automatic"
    mkdir -p "$dir"
    : >"$dir/one"
    : >"$dir/two"
    cat >"$dir/Makefile" <<'EOF'
all: one two
	@printf '%s\n' '$^'
EOF
    out=$(cd "$dir" && "$make_bin")
    [ "$out" = "one two" ] || fail "automatic: $out"
}

case_failure() {
    dir="$work/failure"
    mkdir -p "$dir"
    cat >"$dir/Makefile" <<'EOF'
all:
	false
EOF
    if (cd "$dir" && "$make_bin"); then
        fail "failure exited zero"
    fi
}

case_dry_run() {
    dir="$work/dry-run"
    mkdir -p "$dir"
    cat >"$dir/Makefile" <<'EOF'
all:
	touch made
EOF
    out=$(cd "$dir" && "$make_bin" -n)
    [ "$out" = "touch made" ] || fail "dry-run: $out"
    [ ! -e "$dir/made" ] || fail "dry-run created made"
}

case_silent() {
    dir="$work/silent"
    mkdir -p "$dir"
    cat >"$dir/Makefile" <<'EOF'
all:
	printf 'quiet\n'
EOF
    out=$(cd "$dir" && "$make_bin" -s)
    [ "$out" = "quiet" ] || fail "silent: $out"
}

case_unsupported_parallel() {
    if "$make_bin" -j2 >/dev/null 2>&1; then
        fail "-j2 was accepted"
    fi
}

case_default_goal() {
    dir="$work/default"
    mkdir -p "$dir"
    cat >"$dir/Makefile" <<'EOF'
first:
	@printf 'first\n'
second:
	@printf 'second\n'
EOF
    out=$(cd "$dir" && "$make_bin")
    [ "$out" = "first" ] || fail "default goal: $out"
}

case_variables
case_pattern
case_suffix
case_phony
case_include
case_automatic
case_failure
case_dry_run
case_silent
case_unsupported_parallel
case_default_goal
echo "make: ok"
