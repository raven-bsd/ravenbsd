// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S177/S195 test-only ssh-hostkeys helper. The release service runs `/bin/rsh -c "mkdir ...;
// ssh-keygen ..."`, but the current rsh crashes with a page fault at rip=0 (K514/S44 signal
// handover) and the key directory is never created. This helper creates /tmp/ssh directly and
// execs the staged static ssh-keygen; it does not change the SSH binaries or the product.

#include <errno.h>
#include <sys/stat.h>
#include <unistd.h>

int main(void) {
    if (mkdir("/tmp/ssh", 0700) != 0 && errno != EEXIST) {
        return 1;
    }
    char *args[] = {
        "/bin/ssh-keygen", "-q", "-t", "ed25519", "-N", "",
        "-f", "/tmp/ssh/ssh_host_ed25519_key", (char *)0,
    };
    execv(args[0], args);
    return 1;
}
