#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K531 serial fixture: the exact command sequence `tests/qtest/cases/tmp-writable.toml` sends to
# the release image's root console. Current /tmp is 0755, so the non-root create fails; the expected
# 1777 rows are red until the image assembly is fixed.

stat -c '%a %U %G' /tmp
su raven -c 'touch /tmp/k531-user && echo user-create-ok'
rm -f /tmp/k531-root
touch /tmp/k531-root
chmod 600 /tmp/k531-root
su raven -c 'rm /tmp/k531-root' || echo user-remove-denied
su raven -c 'mkdir /tmp/k531-dir && touch /tmp/k531-dir/mode600 && chmod 600 /tmp/k531-dir/mode600 && stat -c "dir-mode=%a" /tmp/k531-dir && stat -c "file-mode=%a" /tmp/k531-dir/mode600'
rm -rf /tmp/k531-root /tmp/k531-user /tmp/k531-dir && echo root-remove-ok
