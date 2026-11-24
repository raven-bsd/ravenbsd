# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# RavenBSD top-level Makefile. All logic lives in the Rust `xtask` crate; this file only forwards
# targets and variables. `make help` lists every target.
#
# Variables: ARCH=x86_64|aarch64|riscv64 (default x86_64), PLATFORM=generic|pi4, PROFILE=base,
# RELEASE=1 -> --release, VERBOSE=1 prints the commands xtask runs.

ARCH ?= x86_64
PLATFORM ?= generic
PROFILE ?= base
RELEASE ?= 0
VERBOSE ?= 0
SETUP_ARGS ?=
SOAK_SECONDS ?= 120
SOAK_INTERVAL ?= 5
SOAK_PROFILE ?= release
TCP_SOAK_SECONDS ?= 600
TCP_SOAK_INTERVAL ?= 10
BASE ?= origin/master
HELP_SCRIPT ?= scripts/make-help.sh
HELP_MANIFEST ?= scripts/help.txt
SPACE_MIN_GB ?= 20
PRUNE_APPLY ?= 0
PRUNE_DAYS ?= 14
PRUNE_DEPS ?= 0

export VERBOSE

RELFLAG = $(if $(filter 1,$(RELEASE)),--release,)
XTASK = cargo run -q -p xtask --

# firmware paths discovered by `make setup` (scripts/doctor.sh writes this file)
-include .raven-env.mk

.DEFAULT_GOAL := help
.PHONY: help help-xtask setup doctor check check-full check-full-if-needed guard kernel loader brush dev release image run test world provenance \
	posix-gaps posix-check toolchain toolchain-dist toolchain-dist-test clean distclean check-fast check-build ufs-selftest qemu-ufs soak-smoke soak-24h \
	tcp-soak-smoke tcp-soak-24h info space prune

help:
	@sh $(HELP_SCRIPT) "RavenBSD" "$(HELP_MANIFEST)"

help-xtask:
	@$(XTASK) help

setup:
	@sh scripts/setup.sh $(SETUP_ARGS)

doctor:
	@sh scripts/doctor.sh

check:
	@$(XTASK) check

# Fast per-edit gate: guard, fmt, clippy and nextest only for the packages touched by the branch
# or the working tree. Seconds, not minutes. The full `make check` stays the checkpoint gate.
check-fast:
	@$(XTASK) check --fast --changed

# Pre-tag gate: compile every host workspace test target, do not run tests. Closes the
# check-fast changed-package blind spot (K394/K400). Run after check-fast and before the qtests.
check-build:
	@sh tests/release/workspace-build/check.sh

# Host-only UFS image/partition updater proof: GPT/ESP+UFS, put/rm/mkdir/sync,
# fsck and the safety refusals. No board and no whole-image copy.
ufs-selftest:
	@sh scripts/raven-ufs-selftest.sh

# Full allocator stress gate: the 1M-op model runs are ignored by the routine
# `cargo test` in `check` and are executed here, at merge/release time.
check-full:
	@$(XTASK) check
	@cargo test -p raven-libc-malloc --test model -- --ignored --nocapture

# Run the expensive full allocator stress only when the allocator crate or its model test
# changes. The platform seam is faked in this host test, so platform changes belong to qtest.
# 0.1.0 release gate: full nextest/provenance check plus the QEMU login qtest.
# The release profile's user programs are built with the release profile; the debug
# profile trips a known sha2/LLVM debug-info failure and is never a release artifact.
release-gate:
	@$(XTASK) check
	@RAVEN_USER_RELEASE=1 $(XTASK) qtest --case release-login --arch x86_64

soak-24h:
	@sh scripts/soak.sh --duration 86400 --interval 30 --profile $(SOAK_PROFILE)

check-full-if-needed:
	@changed=$$(git diff --name-only $(BASE)...HEAD -- \
		libc/ravenlibc/crates/raven-libc-malloc); \
	if [ -n "$$changed" ]; then \
		printf '%s\n' "$$changed"; \
		$(MAKE) check-full; \
	else \
		printf '%s\n' "check-full-if-needed: no allocator changes since $(BASE); skipped"; \
	fi

guard:
	@$(XTASK) guard

kernel:
	@$(XTASK) build kernel --arch $(ARCH) $(RELFLAG)

loader:
	@$(XTASK) build loader --arch $(ARCH) $(RELFLAG)

brush:
	@$(XTASK) build brush --arch $(ARCH) $(RELFLAG)

dev:
	@sh scripts/build.sh dev $(ARCH)

release:
	@sh scripts/build.sh release $(ARCH)

# Release, image and CI-style builds do not use incremental state; leaving it on only grows
# target/ by tens of gigabytes. Developer builds keep incremental enabled.
image release world release-gate check-build: export CARGO_INCREMENTAL=0

image:
	@sh scripts/space-guard.sh $(SPACE_MIN_GB)
	@$(XTASK) image --arch $(ARCH) --platform $(PLATFORM) --profile $(PROFILE)

# K176: QEMU UFS-root preflight (ESP bootstrap + a real UFS2 root disk).
qemu-ufs:
	@sh scripts/qemu-ufs.sh

soak-smoke:
	@sh scripts/soak.sh --duration $(SOAK_SECONDS) --interval $(SOAK_INTERVAL) --profile $(SOAK_PROFILE)

# K285: bounded 10-minute TCP soak smoke on the tcp-soak profile image. The 24 h run is
# owner-only: `timeout 90000 make tcp-soak-24h`.
tcp-soak-smoke:
	@sh scripts/tcp-soak.sh --duration $(TCP_SOAK_SECONDS) --interval $(TCP_SOAK_INTERVAL)

tcp-soak-24h:
	@sh scripts/tcp-soak.sh --duration 86400 --interval 30

run:
	@$(XTASK) run --arch $(ARCH) --profile $(PROFILE)

test:
	@$(XTASK) check
	@$(XTASK) qtest --all

world:
	@$(XTASK) libc-world build --all

provenance:
	@cargo run -q -p raven-provenance -- check --root .

# Code statistics: Rust code lines first, then the full tokei language table. The scan excludes
# build output, Git, and vendor trees; the summary separates vendored uutils/brush from our code.
info:
	@sh scripts/info.sh

# Disk use and safe build-cache pruning. `prune` is dry-run until PRUNE_APPLY=1.
space:
	@sh scripts/space.sh

prune:
	@sh scripts/prune.sh $(if $(filter 1,$(PRUNE_APPLY)),--apply,) --days $(PRUNE_DAYS) $(if $(filter 1,$(PRUNE_DEPS)),--deps,)

posix-gaps:
	@$(XTASK) posix-gaps

posix-check:
	@$(XTASK) posix-check --profile posix --arch $(ARCH)

TOOLCHAIN_STAGE ?= 1

toolchain:
	@TOOLCHAIN_STAGE=$(TOOLCHAIN_STAGE) sh toolchain/build.sh

toolchain-dist:
	sh toolchain/dist.sh

toolchain-dist-test:
	sh toolchain/test-dist.sh

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
