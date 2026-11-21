#!/bin/brush
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K414: enumerate every uutils source directory (excluding the shared checksum_common
# module) and prove that every staged binary is executable in /bin. A staged name that
# is missing prints MISSING and fails the inventory; names that are not built yet are
# reported as `gap` (the product ticket list, not a silent omission).

all="arch b2sum base32 base64 basename basenc cat chcon chgrp chmod chown chroot cksum comm cp csplit cut date dd df dir dircolors dirname du echo env expand expr factor false fmt fold groups head hostid hostname id install join kill link ln logname ls md5sum mkdir mkfifo mknod mktemp more mv nice nl nohup nproc numfmt od paste pathchk pinky pr printenv printf ptx pwd readlink realpath rm rmdir runcon seq sha1sum sha224sum sha256sum sha384sum sha512sum shred shuf sleep sort split stat stdbuf stty sum sync tac tail tee test timeout touch tr true truncate tsort tty uname unexpand uniq unlink uptime users vdir wc who whoami yes"
must="ls cat cp mv rm ln mkdir rmdir touch stat df du echo pwd basename dirname true false sleep date head tail wc sort cut tr env id chmod chown"

total=0
staged=0
gaps=0
missing=0
for name in $all; do
    total=$((total + 1))
    if [ -x "/bin/$name" ]; then
        echo "uutils: $name ok"
        staged=$((staged + 1))
    else
        echo "uutils: $name gap"
        gaps=$((gaps + 1))
    fi
done

for name in $must; do
    if [ ! -x "/bin/$name" ]; then
        echo "uutils: $name MISSING"
        missing=$((missing + 1))
    fi
done

echo "uutils: total=$total staged=$staged gaps=$gaps"
if [ "$missing" -eq 0 ]; then
    echo "uutils: inventory ok"
else
    echo "uutils: inventory FAILED missing=$missing"
    exit 1
fi
