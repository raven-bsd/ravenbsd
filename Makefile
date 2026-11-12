# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# RavenBSD top-level Makefile. All logic lives in the Rust `xtask` crate; this file only forwards
# targets and variables. `make help` lists every target.
#
# Variables: ARCH=x86_64|aarch64|riscv64 (default x86_64), PROFILE=base, RELEASE=1 -> --release,
# VERBOSE=1 prints the commands xtask runs.

ARCH ?= x86_64
PROFILE ?= base
RELEASE ?= 0
VERBOSE ?= 0
SETUP_ARGS ?=

export VERBOSE

RELFLAG = $(if $(filter 1,$(RELEASE)),--release,)
XTASK = cargo run -q -p xtask --

# firmware paths discovered by `make setup` (scripts/doctor.sh writes this file)
-include .raven-env.mk

.DEFAULT_GOAL := help
.PHONY: help setup doctor check guard kernel loader dev release image run test world provenance \
	toolchain clean distclean

help:
	@$(XTASK) help

setup:
	@sh scripts/setup.sh $(SETUP_ARGS)

doctor:
	@sh scripts/doctor.sh

check:
	@$(XTASK) check

guard:
	@$(XTASK) guard

kernel:
	@$(XTASK) build kernel --arch $(ARCH) $(RELFLAG)

loader:
	@$(XTASK) build loader --arch $(ARCH) $(RELFLAG)

dev:
	@sh scripts/build.sh dev $(ARCH)

release:
	@sh scripts/build.sh release $(ARCH)

image:
	@$(XTASK) image --arch $(ARCH) --profile $(PROFILE)

run:
	@$(XTASK) run --arch $(ARCH) --profile $(PROFILE)

test:
	@$(XTASK) check
	@$(XTASK) qtest --all

world:
	@printf '%s\n' 'not implemented yet (milestone M4)' >&2; exit 1

provenance:
	@cargo run -q -p raven-provenance -- check --root .

TOOLCHAIN_STAGE ?= 1

toolchain:
	@TOOLCHAIN_STAGE=$(TOOLCHAIN_STAGE) sh toolchain/build.sh

clean:
	@echo "removing build output"
	@if command -v cargo >/dev/null 2>&1; then cargo clean 2>/dev/null || true; fi
	@rm -rf target build .raven-env.mk
	@echo "clean"

distclean:
	@echo "removing build output and local caches"
	@if command -v cargo >/dev/null 2>&1; then cargo clean 2>/dev/null || true; fi
	@rm -rf target build vendor .raven-env.mk
	@echo "distclean"
