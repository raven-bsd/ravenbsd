#!/bin/brush
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K217: the complete eink-insult proof in one staged script. The qtest sends this script once;
# every result prints a unique marker, so the serial wait does not depend on per-character
# prompt redraws or multi-command typing. The expected body comes from insult.txt staged at
# /etc/eink-insult.txt; this script does not hard-code the insult.

base=http://10.0.2.2:18081
expected=/etc/eink-insult.txt
curl_out=/tmp/insult-curl.txt
wget_out=/tmp/insult-wget.txt

body=$(/bin/curl -s "$base/insult")
echo "eink-body: $body"

/bin/curl -s -o "$curl_out" "$base/insult"
/bin/wget -q -O "$wget_out" "$base/insult"

curl_bytes=$(/bin/wc -c < "$curl_out")
wget_bytes=$(/bin/wc -c < "$wget_out")
expected_bytes=$(/bin/wc -c < "$expected")
curl_text=$(/bin/cat "$curl_out")
wget_text=$(/bin/cat "$wget_out")
expected_text=$(/bin/cat "$expected")
if [ "$curl_bytes" = "$wget_bytes" ] && [ "$curl_bytes" = "$expected_bytes" ] && [ "$curl_text" = "$expected_text" ] && [ "$wget_text" = "$expected_text" ]; then
    echo "eink-cmp: ok"
else
    echo "eink-cmp: FAIL"
    echo "eink-done"
    exit 1
fi

status=$(/bin/curl -s -I "$base/missing" | /bin/head -n 1)
echo "eink-404: $status"

/bin/curl -s -f "$base/missing"
echo "eink-f: $?"

redirect=$(/bin/curl -s -L "$base/redirect")
echo "eink-redirect: $redirect"

echo "eink-done"
exit 0
