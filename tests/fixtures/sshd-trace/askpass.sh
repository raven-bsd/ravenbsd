#!/bin/brush
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K498 SSH_ASKPASS helper for the guest-loopback password login. The trace
# image has no /bin/chmod; the overlay installs this file as 0755 so ssh can
# exec it with SSH_ASKPASS_REQUIRE=force. It is a test fixture, not a product
# credential: the image password is already documented in profiles/ssh.d.
echo raven
