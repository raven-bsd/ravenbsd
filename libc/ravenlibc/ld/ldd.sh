#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Print the dependency trace of each dynamic program; static programs exit 1.

for program in "$@"; do
    LD_TRACE_LOADED_OBJECTS=1 /libexec/ld-elf.so.1 --list "$program"
done
