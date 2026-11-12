# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

XTASK ?= cargo run -q -p xtask --
.PHONY: libc-lint libc-check
libc-lint:
	@$(XTASK) libc-lint
libc-check: libc-lint
	@$(XTASK) check
