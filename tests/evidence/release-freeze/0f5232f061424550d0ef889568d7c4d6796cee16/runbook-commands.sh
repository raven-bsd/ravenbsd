#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K384: extracted shell commands from the freeze checklist. This file is parsed with
# `sh -n` and is not executed. Heavy gates are QUIET-CHECKPOINT; see the checklist.

make check
cargo xtask qtest --case brush-login --arch x86_64
cargo xtask qtest --case login-latency --arch x86_64
sh scripts/qemu-net.sh
cargo xtask qtest --case disk-tools --arch x86_64
sh scripts/ufs-demo.sh
sh scripts/pi4-install.sh --check
sh scripts/pi4-release.sh --device /dev/ttyUSB0
sh scripts/pi4-hammer.sh --board --device /dev/ttyUSB0
sh scripts/board-pi4-check.sh --log build/release/0.1.0/pi4-serial.log
sh scripts/pi4-update.sh --check
sh scripts/fault-injection.sh --profile release --arch x86_64 --soak 86400
sh tests/scripts/check-soak-24h.sh --self-test
sh scripts/release-evidence.sh --self-test
sh scripts/release-evidence.sh
sh scripts/release-evidence.sh --check
cargo run -q -p raven-atf-run -- gate --g1
cargo xtask posix-check --profile posix --arch x86_64
cargo xtask posix-gaps --check
sh scripts/kernel-gate.sh
cargo xtask qtest --case dynamic-userland --arch x86_64
cargo xtask qtest --case dynamic-exec-aarch64 --arch aarch64
cargo xtask qtest --case release-login --arch x86_64
cargo xtask qtest --case posix-shell --arch x86_64
cargo xtask qtest --case libc-spawn --arch x86_64
make image ARCH=x86_64 PROFILE=release
# cargo xtask qtest --case hdmi-console --arch x86_64  # HDMI1 to write; missing
# sh scripts/pi4-eink-evidence.sh  # E1/E3 to write; missing
