# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S785 diagnostic TAP script: locate the guest command that triggers the Rust TLS abort.
# Each marker is printed before the step so the serial log names the last completed step.

echo "diag: echo"
/bin/true
echo "diag: true"
x=`echo backquote`
echo "diag: backquote=$x"
/bin/dirname /a/b
echo "diag: dirname"
/bin/tail -1 /etc/mounts
echo "diag: tail"
/bin/grep -E ok /etc/mounts
echo "diag: grep"
/bin/sleep 0
echo "diag: sleep"
/tests/pjdfstest/pjdfstest lstat /etc/mounts type
echo "diag: helper"
echo "1..1"
echo "ok 1"
