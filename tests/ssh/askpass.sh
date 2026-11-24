#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K434 SSH_ASKPASS helper: the matrix runs the host OpenSSH client without a controlling tty and
# supplies the image password through this script. Never used outside the test harness.

printf '%s\n' "${K434_SSH_PASSWORD:-raven}"
