#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
res= tc= list=0
while [ $# -gt 0 ]; do case "$1" in -l) list=1; shift;; -r) res=$2; shift 2;; -s|-v) shift 2;; -*) shift;; *) tc=$1; shift;; esac; done
if [ "$list" = 1 ]; then printf 'Content-Type: application/X-atf-tp; version="1"\n\nident: env\n'; exit 0; fi
if [ "$TZ" = UTC ] && [ "$LANG" = C.UTF-8 ] && [ "$LC_ALL" = C.UTF-8 ] && [ -d "$HOME" ] && [ -d "$TMPDIR" ] && [ -z "$RL_TEST_LEAK" ]; then
  printf 'passed\n' > "$res"
else
  printf 'failed: environment leaked\n' > "$res"
  exit 1
fi
