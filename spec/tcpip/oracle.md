<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Behaviour oracle: measurement plan and result table

Status: plan for C24, with the measurements that could actually be taken on the
authoring host. C24 owns `oracle-results.md`; this file defines how to fill it
and what each row means. The reference stack is measured as a running system.
No reference-stack source is read in C24; that is the clean-room line.

## 1. Rules

1. A row is `OBSERVED` only with the command and the output (or the capture
   file) recorded next to it. A value copied from documentation, memory or a
   source file is not an observation.
2. A row that cannot be measured on the available host is `NOT-MEASURED` with
   the reason. It is never guessed and never silently dropped.
3. A difference is `DEFECT` until the reviewer classifies it as
   `DEVIATION-ACCEPTED`. C24 does not fix defects.
4. Every `DEVIATION-ACCEPTED` gets a regression test in `tests/tcpip/` and an
   entry in `features.md` §13.
5. The comparison is between **observable behaviour**: packets on the wire,
   socket state and timing. It is not a code comparison.

## 2. Reference stack and capture method

Reference stack used on the authoring host:

```
$ uname -a
Linux alien 5.19.0-50-generic #50-Ubuntu SMP PREEMPT_DYNAMIC Mon Jul 10 18:24:29 UTC 2023 x86_64
```

Linux 5.19 is the host kernel; its TCP is the reference for the rows measured
here. A second reference (a NetBSD or FreeBSD guest) is desirable for C24 but
was not available during C01; C24 records whichever stacks it can run.

Three observation channels are used:

| Channel | What it gives | Availability |
|---|---|---|
| `tcpdump -i lo -nn -s 0 -w cap.pcap 'tcp'` | full bytes: option order, flags, MSS, window, RST vs silence | **NOT available on the authoring host**: `tcpdump: lo: You don't have permission to capture on that device`. C24 needs root or `CAP_NET_RAW`. |
| `ss -tinm '( sport = :P or dport = :P )'` | state, cwnd, ssthresh, rto, rtt/rttvar, mss, advmss, wscale, cc algorithm, `ato` (delayed-ACK timeout) | Available unprivileged. Used below. |
| `nstat -az Tcp*` | MIB counters: opens, fails, resets, in/out segments, retransmissions, errors, out RSTs | Available unprivileged. Used below. |
| stack trace (C04) | our side, same fields and same event names | C04 harness |

A capture command for C24, once capture permission exists:

```
sudo tcpdump -i lo -nn -s 0 -w /tmp/opencode/tcp-oracle.pcap 'tcp port 4242'
sudo tcpdump -nn -r /tmp/opencode/tcp-oracle.pcap -vv -X
```

The analyser must print, for SYN, SYN-ACK and the first data segment: option
kinds in wire order, MSS, window scale shift and the raw window field (which is
unscaled on SYN), SACK-permitted presence, TSval/TSecr presence, ECN flags,
initial sequence number, and the IP/TCP length. For a closed port: whether a
RST came back or the segment was silently dropped. For a bad-sequence segment:
RST, ACK or silence.

Our side is captured by C04's trace recorder in the same logical format, so the
two columns can be compared without reading either implementation.

## 3. Measurements actually taken on this host

These are `OBSERVED` with the command shown. The helper was a short Python
script that creates a loopback server and client, reads `TCP_MAXSEG`, runs
`ss -tinm` and `nstat -az`, closes the client first, and polls `ss -tan` until
the four-tuple disappears. It is a measurement harness, not a network stack.

Command shape:

```
python3 /tmp/opencode/oracle_measure.py
```

### 3.1 Host configuration (OBSERVED)

```
net.ipv4.tcp_timestamps = 1
net.ipv4.tcp_window_scaling = 1
net.ipv4.tcp_sack = 1
net.ipv4.tcp_ecn = 2
net.ipv4.tcp_rmem = 4096  131072  6291456
net.ipv4.tcp_wmem = 4096  16384   4194304
net.core.somaxconn = 4096
net.ipv4.tcp_keepalive_time = 7200
net.ipv4.tcp_keepalive_intvl = 75
net.ipv4.tcp_keepalive_probes = 9
net.ipv4.tcp_fin_timeout = 60
net.ipv4.tcp_max_syn_backlog = 2048
net.ipv4.tcp_syn_retries = 6
net.ipv4.tcp_retries2 = 15
net.ipv4.ip_local_port_range = 32768 60999
net.ipv4.tcp_abort_on_overflow = 0
```

These are configuration values, not behaviour; they tell C24 what knobs the
reference was using.

### 3.2 Established loopback connection, IPv4 (OBSERVED)

```
$ ss -tinm '( sport = :42919 or dport = :42919 )'
ESTAB 0 0 127.0.0.1:39180 127.0.0.1:42919
  skmem:(r0,rb131072,t0,tb2626560,f4096,w0,o0,bl0,d0) cubic wscale:7,7
  rto:200 rtt:0.046/0.023 mss:32741 pmtu:65535 rcvmss:536 advmss:65483
  cwnd:10 bytes_acked:1 segs_out:2 segs_in:1 ... app_limited
  rcv_space:65495 rcv_ssthresh:65495 minrtt:0.046
```

Observed: `cwnd:10` (initial window of 10 segments, RFC 6928), `wscale:7,7`,
`rto:200` (the configured 200 ms floor), `cubic` as the congestion algorithm,
`ato:40` on the receiver after one byte (40 ms delayed-ACK timer), and
`advmss:65483` on loopback. The `mss` field shown by `ss` is the
`TCP_MAXSEG` clamp, not the on-wire MSS; `advmss` is the value to compare with
our announced MSS.

### 3.3 Established loopback connection, IPv6 (OBSERVED)

```
$ ss -tinm '( sport = :36347 or dport = :36347 )'
ESTAB ... [::1]:36996 [::1]:36347
  ... cubic wscale:7,7 rto:204 rtt:0.023/0.011 mss:32732 pmtu:65536
  rcvmss:536 advmss:65464 cwnd:10 ... ato:40 ... rcv_space:65476
```

Observed: IPv6 loopback announces `advmss:65464`, IPv4 announces `65483`.
The difference is 19 bytes on this kernel (one byte less than the 20-byte IP
header delta because the loopback MTU is 65536 for v6 and the v4 path uses
65535); C24 should compare the exact delta, not a hardcoded 20.

### 3.4 Closed port (OBSERVED)

```
$ python3 - <<'PY'
import socket
s=socket.socket(); s.settimeout(2)
try: s.connect(("127.0.0.1",1))
except OSError as e: print("errno", e.errno, e.strerror)
PY
errno 111 Connection refused
```

Observed: a TCP connection to a closed port is refused, i.e. a RST is
returned. This does not distinguish a RST from a silent drop with a local
timeout; the immediate `ECONNREFUSED` (not `ETIMEDOUT`) does.

### 3.5 TIME-WAIT duration (OBSERVED)

```
$ ss -tan '( sport = :41802 or dport = :41802 )'
TIME-WAIT 0 0 127.0.0.1:41802 127.0.0.1:42573
time until the 4-tuple disappeared: 62.3 s
```

Observed: the active closer stays in TIME-WAIT for about 62 s on this host
(`tcp_fin_timeout = 60`, poll granularity 1 s). Our default in
`observability.md` is 60 s, which is within the measurement's resolution.

### 3.6 MIB counters (OBSERVED)

```
$ nstat -az TcpActiveOpens TcpPassiveOpens TcpAttemptFails TcpEstabResets \
    TcpInSegs TcpOutSegs TcpRetransSegs TcpInErrs TcpOutRsts
TcpActiveOpens      343835
TcpPassiveOpens     215627
TcpAttemptFails      41379
TcpEstabResets       17825
TcpInSegs         40198216
TcpOutSegs        48965521
TcpRetransSegs       71752
TcpInErrs             1001
TcpOutRsts          342845
```

These confirm that the MIB-style counter set named in `observability.md` is the
operational set an operator actually reads. They are cumulative host values,
not a controlled experiment; C24 must reset/delta them around its own test.

## 4. Rows measured, partially measured, and not measurable here

| Behaviour | Reference value on this host | Status | Evidence |
|---|---|---|---|
| Initial congestion window | 10 segments (`cwnd:10`) | OBSERVED | §3.2, §3.3 |
| Congestion algorithm | CUBIC (`cubic`) | OBSERVED | §3.2 |
| Initial RTO | 200 ms (`rto:200`) | OBSERVED | §3.2 |
| Delayed-ACK timer | 40 ms (`ato:40` after one byte) | OBSERVED | §3.2, §3.3 |
| Window scale | 7,7 on loopback | OBSERVED | §3.2 |
| MSS announced, IPv4 | 65483 bytes on loopback | OBSERVED | §3.2 |
| MSS announced, IPv6 | 65464 bytes on loopback | OBSERVED | §3.3 |
| Closed port | RST, immediate `ECONNREFUSED` | OBSERVED | §3.4 |
| TIME-WAIT duration | ~62 s | OBSERVED | §3.5 |
| MIB counter names | present and incrementing | OBSERVED | §3.6 |
| Option set and order on SYN/SYN-ACK/first data | — | NOT-MEASURED: no packet-capture permission (`tcpdump: lo: You don't have permission to capture on that device`) | C24 must rerun with `CAP_NET_RAW` or a TAP |
| Initial receive window (raw, unscaled) | — | NOT-MEASURED: requires the SYN capture | same |
| RST vs silent drop for wrong sequence, out-of-window, TIME-WAIT | — | NOT-MEASURED: requires a raw-injection harness and capture | C20/C24 |
| Challenge-ACK behaviour and rate limit | — | NOT-MEASURED: requires injection and capture | C24 |
| Keepalive onset/interval/probe count | configured 7200 s / 75 s / 9, not exercised | PARTIAL: configuration OBSERVED, behaviour NOT-MEASURED | §3.1 |
| FIN-WAIT-2 duration | `tcp_fin_timeout = 60`, not exercised | PARTIAL: configuration OBSERVED, behaviour NOT-MEASURED | §3.1 |
| Zero-window probe cadence and backoff | — | NOT-MEASURED: requires a receiver that stops reading and a capture | C24 |
| ECN negotiation and CE response | `tcp_ecn = 2` (offer), not exercised | PARTIAL: configuration OBSERVED, negotiation NOT-MEASURED | §3.1 |
| Simultaneous open | — | NOT-MEASURED: needs two controlled endpoints and capture | C24 |
| Simultaneous close | — | NOT-MEASURED: needs two controlled endpoints and capture | C24 |
| PMTU / PLPMTUD behaviour | — | NOT-MEASURED: needs a path with a smaller MTU and ICMP filtering | C24 |
| SYN-flood / listen queue behaviour | `tcp_max_syn_backlog = 2048`, `tcp_abort_on_overflow = 0` | PARTIAL: configuration only | §3.1 |

## 5. Table C24 fills

C24 writes `oracle-results.md` with one row per behaviour and these columns:

| Column | Meaning |
|---|---|
| `ID` | Stable row ID (`OR-01` ...) |
| `Behaviour` | One behaviour, narrow enough to be measured |
| `Reference stack` | Name, version, kernel/OS |
| `Reference value` | Observed value with unit |
| `Our value` | Observed value from our C04 trace or socket info |
| `Verdict` | `MATCH`, `DEVIATION-ACCEPTED`, `DEFECT`, or `NOT-MEASURED` |
| `Evidence` | Command + capture file, or transcript; a row with no evidence is invalid |
| `Regression test` | Test name for an accepted deviation; required for `DEVIATION-ACCEPTED` |
| `Defect ticket` | Ticket ID for `DEFECT`; required for `DEFECT` |

Required rows (from C24 §4): option set and order on SYN, SYN-ACK and first
data; initial window and initial RTO; delayed-ACK delay and immediate-ACK
conditions; duplicate-ACK threshold and time to fast retransmit; RST versus
silent drop for closed port, wrong sequence, out-of-window, TIME-WAIT; challenge
ACK and its rate limit; keepalive onset/interval/probe count; FIN-WAIT-2 and
TIME-WAIT durations; zero-window probe cadence; MSS for v4 and v6; ECN
negotiation and CE response; simultaneous open; simultaneous close.

## 6. Capture analysis checklist

For each SYN, SYN-ACK and first data segment, record:

* TCP option kinds in wire order, with lengths; mark NOP/EOL padding.
* MSS value; window scale offer; SACK-permitted; TSval/TSecr; ECN flags
  (ECE/CWR and IP ECT/CE).
* Raw 16-bit window field; whether it is scaled (SYN must not be).
* Sequence and acknowledgment numbers; whether the ACK acknowledges the SYN.
* IP DF bit and total length; fragment flags on v4; flow label on v6.

For RST rows, record whether a packet was emitted at all, and its
`SEQ`/`ACK`/flags. A capture that shows silence is evidence only if the
capture filter covered both directions and the timeout exceeded one RTO.

For timing rows, the capture must contain both the trigger and the response,
and the clock must be the capture's own timestamp; do not time from a
userspace `sleep`.

## 7. What C24 must not do

* Do not read the reference stack's source to explain a difference.
* Do not record a value that was not observed.
* Do not present a simulated trace as a reference measurement.
* Do not classify a difference as an accepted deviation without the reviewer.
* Do not fix a defect in C24; file it in the owning lane and reference it.
