#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
# Golden ATF listing of ticket C71 4.7.2.
res= tc= list=0
while [ $# -gt 0 ]; do
  case "$1" in
    -l) list=1; shift ;;
    -r) res=$2; shift 2 ;;
    -s|-v) shift 2 ;;
    -*) shift ;;
    *) tc=$1; shift ;;
  esac
done
if [ "$list" = 1 ]; then
  printf 'Content-Type: application/X-atf-tp; version="1"\n\n'
  printf 'ident: ok_case\ndescr: passes\n\n'
  printf 'ident: needs_root\nrequire.user: root\n\n'
  printf 'ident: sleeper\ntimeout: 1\n'
  exit 0
fi
case "$tc" in
  ok_case) printf 'passed\n' > "$res" ;;
  needs_root) printf 'skipped: root required\n' > "$res" ;;
  sleeper) sleep 3; printf 'passed\n' > "$res" ;;
esac
