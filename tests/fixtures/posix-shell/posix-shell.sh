# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P4a POSIX shell conformance script. It prints one marker per requirement family and is
# run by brush --posix on the host and through the posix-shell qtest on the target. It uses
# builtins only; no pipeline or job-control assertion is included (PG-35/PG-56).

x=1; y=2
echo simple:$x:$y
false
echo status:$?
true && echo and:ok
false || echo or:ok
{ echo group:ok; }
( echo subshell:ok )
f() { echo func:$1; return 3; }
f arg
echo func-status:$?
if true; then echo if:ok; fi
case abc in a*) echo case:ok;; esac
for i in 1 2; do echo for:$i; done
n=0
while [ "$n" -lt 2 ]; do n=$((n+1)); echo while:$n; done
until [ "$n" -ge 2 ]; do n=$((n+1)); done
echo until:$n
for i in 1 2 3; do echo break-loop:$i; break; done
echo break:done
for i in 1 2 3; do if [ "$i" = 2 ]; then continue; fi; echo continue:$i; done
cd /tmp
echo cd:$?
cd /
command echo cmd:ok
echo command:$?
export P4B=yes
echo export:$P4B
hash
echo hash:$?
set -- a b
shift
echo shift:$#:$1
test -n x
echo test:$?
[ -n x ]
echo bracket:$?
type echo
echo type:$?
alias p4b="echo alias:ok"
alias p4b
echo alias:$?
unalias p4b
echo unalias:$?
f() { return 7; }
f
echo return:$?
echo 'echo dot:ok' > /tmp/p4b-dot
. /tmp/p4b-dot
v=abc
echo p1:${v:-x}:${v#a}:${v%c}:${#v}
unset u
echo p2:${u:-default}
echo arith:$((2+3))
echo sub:$(echo inner)
IFS=:
v=a:b:c
set -- $v
echo split:$#:$1:$2:$3
unset IFS
echo a > /tmp/p4g-a
echo b > /tmp/p4g-b
echo glob
echo /tmp/p4g-*
echo "quote:a b"
echo redir > /tmp/p4-r
read line < /tmp/p4-r
echo read:$line
IFS= read here <<EOF
here:body
EOF
echo here:$here
echo close >&- 2>&-
echo after-close:$?
set -e
set +e
( set -e; false; echo unreachable )
echo errexit:$?
( set -u; echo "$missing" )
echo nounset:$?
( set -x; echo traced )
echo xtrace:$?
trap "echo trap:exit" EXIT
eval "echo eval:ok"
readonly ro=1
echo ro:$ro
umask 022
echo umask:$(umask)
set -- -a -b val x
while getopts ab: o; do echo getopt:$o:$OPTARG; done
echo optind:$OPTIND
echo pid:$$
echo done
