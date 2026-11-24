#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# SSH_ASKPASS helper for the S179 release-image conformance run. The harness exports
# RAVEN_CONFORMANCE_PASSWORD; OpenSSH invokes this script with the prompt as its argument.
printf '%s\n' "${RAVEN_CONFORMANCE_PASSWORD:-raven}"
