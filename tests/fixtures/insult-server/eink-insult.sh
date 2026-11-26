#!/bin/brush
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K217: the complete eink-insult proof in one staged script. The qtest sends this script once;
# every result prints a unique marker, so the serial wait does not depend on per-character
# prompt redraws or multi-command typing. The expected body comes from insult.txt staged at
# /etc/eink-insult.txt; this script does not hard-code the insult.
#
# This shell has no external command substitution or pipeline support yet, so the script writes
# each command's output to a file and reads it with the builtin `read`. Do not reintroduce
# `$(...)` or `|` here.

base=http://10.0.2.2:18083
expected=/etc/eink-insult.txt
curl_out=/tmp/insult-curl.txt
wget_out=/tmp/insult-wget.txt

printf 'eink-body: '
/bin/curl -s "$base/insult"
echo

/bin/curl -s -o "$curl_out" "$base/insult"
/bin/wget -q -O "$wget_out" "$base/insult"

/bin/wc -c < "$curl_out" > /tmp/curl-bytes
/bin/wc -c < "$wget_out" > /tmp/wget-bytes
/bin/wc -c < "$expected" > /tmp/expected-bytes
read curl_bytes < /tmp/curl-bytes
read wget_bytes < /tmp/wget-bytes
read expected_bytes < /tmp/expected-bytes
/bin/cat "$curl_out" > /tmp/curl-text
/bin/cat "$wget_out" > /tmp/wget-text
/bin/cat "$expected" > /tmp/expected-text
read curl_text < /tmp/curl-text
read wget_text < /tmp/wget-text
read expected_text < /tmp/expected-text
if [ -n "$curl_bytes" ] && [ -n "$wget_bytes" ] && [ -n "$expected_bytes" ] && [ -n "$curl_text" ] && [ -n "$wget_text" ] && [ -n "$expected_text" ] && [ "$curl_bytes" = "$wget_bytes" ] && [ "$curl_bytes" = "$expected_bytes" ] && [ "$curl_text" = "$expected_text" ] && [ "$wget_text" = "$expected_text" ]; then
    echo "eink-cmp: ok"
else
    echo "eink-cmp: FAIL"
    echo "eink-done"
    exit 1
fi

printf 'eink-404: '
/bin/curl -s -I "$base/missing"
echo

/bin/curl -s -f "$base/missing"
echo "eink-f: $?"

printf 'eink-redirect: '
/bin/curl -s -L "$base/redirect"
echo

echo "eink-done"
exit 0
