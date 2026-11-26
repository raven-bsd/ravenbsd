#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# SSH_ASKPASS helper for the S85k gate. OpenSSH calls this when stdout is not a
# terminal; the gate sets SSH_ASKPASS_REQUIRE=force so the password never needs a
# tty and no expect-style password injection touches the client.

printf '%s\n' "${RAVEN_SSH_PASSWORD:?RAVEN_SSH_PASSWORD is not set}"
