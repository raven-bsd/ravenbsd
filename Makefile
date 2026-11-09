# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# RavenBSD top-level Makefile. All logic lives in scripts/ (POSIX sh); this file only calls them.
#
#   make setup     install everything needed on this machine (Ubuntu/Debian or Fedora), then verify
#   make dev       debug build of everything that exists: big binaries, full debug info
#   make release   smallest-possible optimised build (opt-level z, fat LTO, stripped)
#   make clean     remove all build output and start over
#   make check     local gate: stable guard, fmt, clippy, tests (run before asking for review)
#
# Variables: ARCH=x86_64|aarch64|riscv64 (default x86_64); SETUP_ARGS="--dry-run|--check|--donors".

ARCH ?= x86_64
SETUP_ARGS ?=

# firmware paths discovered by `make setup` (scripts/doctor.sh writes this file)
-include .raven-env.mk

.DEFAULT_GOAL := help
.PHONY: help setup doctor dev release clean check

help:
	@printf '%s\n' \
	  'RavenBSD' \
	  '  make setup     install host packages, Rust toolchain, firmware paths; verify (Ubuntu/Debian, Fedora)' \
	  '                 SETUP_ARGS="--dry-run" shows what would run; "--donors" also fetches ../ravenbsd-resources' \
	  '  make dev       debug build (ARCH=$(ARCH)): no optimisation, full debug info, overflow checks' \
	  '  make release   release build (ARCH=$(ARCH)): smallest binaries, fat LTO, stripped' \
	  '  make clean     remove build output (target/, build/) and start over' \
	  '  make check     stable guard, fmt, clippy, tests' \
	  '  make doctor    report what is installed or missing'

setup:
	@sh scripts/setup.sh $(SETUP_ARGS)

doctor:
	@sh scripts/doctor.sh

dev:
	@sh scripts/build.sh dev $(ARCH)

release:
	@sh scripts/build.sh release $(ARCH)

check:
	@sh scripts/check.sh $(ARCH)

clean:
	@echo "removing build output"
	@if command -v cargo >/dev/null 2>&1; then cargo clean 2>/dev/null || true; fi
	@rm -rf target build .raven-env.mk
	@echo "clean"
