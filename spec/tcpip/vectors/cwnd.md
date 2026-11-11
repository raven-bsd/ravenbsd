<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Congestion-window arithmetic

Status: companion to `cwnd.toml` and normative for C13. All values are bytes;
all arithmetic is integer. The vectors pin the value of `cwnd` and `ssthresh`
after each event. A per-ACK implementation may take a different path between
two events, but it must land on the expected value at the event boundary.

## 1. Common definitions

| Symbol | Meaning |
|---|---|
| `MSS` | sender maximum segment size for the sequence (1460 in the vectors) |
| `cwnd` | congestion window, bytes |
| `ssthresh` | slow-start threshold, bytes |
| `initial_cwnd` | cwnd at the start of the sequence |
| `cwnd_max` | configured cap, bytes |
| `FlightSize` | bytes sent but not cumulatively acknowledged |
| `acked` | bytes acknowledged by the event |
| `IW` | initial window, 10 segments (RFC 6928; see `DEVIATION-04`) |

The initial window in the sequences is explicit (`initial_cwnd`); a real
connection starts at `tcp.initial_window * MSS` unless the property is
changed. `cwnd_max` is applied after every update. The `cwnd_min` floor of two
segments applies after recovery; the RTO loss window is one segment until the
first new ACK (RFC 5681 §3.1).

## 2. NewReno

### 2.1 ACK of new data, not in recovery

Per ACK, with appropriate byte counting (RFC 3465):

```
if cwnd < ssthresh:
    cwnd += min(acked, MSS)                 # slow start
else:
    cwnd += max(1, (MSS*MSS) / cwnd)        # congestion avoidance, floor
cwnd = min(cwnd, cwnd_max)
```

An `rtt` step in the vectors is a loop of `acks` calls with `acked = MSS`, in
order, with cwnd changing between calls. This is why the linear-growth sequence
does not simply add `acks*MSS*MSS/cwnd`; the per-ACK floor matters.

### 2.2 Loss and fast recovery

On the duplicate-ACK threshold (`tcp.dup_ack_threshold`, default 3), with
`FlightSize` at the loss event:

```
ssthresh = max(floor(FlightSize/2), 2*MSS)
cwnd     = ssthresh + 3*MSS
in_recovery = true
recover  = SND.NXT            # RFC 6582's recover variable
retransmit the segment at SND.UNA
```

The `loss` event in the vectors supplies `flight` and expects these values. It
does not model the retransmission itself.

### 2.3 Partial and full ACK (RFC 6582 §3.2)

While `in_recovery`, an ACK that acknowledges new data but is below `recover`
is a **partial ACK**:

```
cwnd = max(2*MSS, cwnd - acked + (MSS if acked >= MSS else 0))
retransmit the next hole
```

An ACK at or above `recover` is the **full ACK**:

```
cwnd = ssthresh
in_recovery = false
```

The vector `newreno-partial-ack` shows the counter-intuitive but correct
result: two 1-MSS partial ACKs leave cwnd at 13 MSS because each partial ACK
deflates by 1 MSS and immediately adds 1 MSS back for the retransmission.

### 2.4 RTO

```
if not already in RTO recovery:
    ssthresh = max(floor(FlightSize/2), 2*MSS)
cwnd = MSS
in_recovery = false
```

A second RTO before any new data is acknowledged does **not** cut ssthresh
again (RFC 5681 §3.1, the `in_rto_recovery` flag in the reference arithmetic).
The first new ACK clears the flag; a later RTO cuts ssthresh from the new
FlightSize. The vector `newreno-repeated-rto` pins the second RTO.

### 2.5 Idle restart

```
cwnd = min(cwnd, IW)
```

RFC 5681 §4.1. The vector `newreno-idle-restart` starts at 100 MSS and expects
10 MSS.

### 2.6 Worked steps

`newreno-slow-start`: 10 MSS -> 10 ACKs -> 20 MSS -> 20 ACKs -> 40 MSS.

`newreno-slow-start-to-ca`: after the first RTT cwnd = ssthresh = 20 MSS.
The second RTT has 20 ACKs in congestion avoidance:

```
start 29200
ACK 1: floor(1460*1460/29200) = 73  -> 29273
ACK 2: floor(1460*1460/29273) = 72  -> 29345
ACK 3: floor(1460*1460/29345) = 72  -> 29417
...
20 ACKs in total -> 30617
```

`newreno-fast-retransmit`: FlightSize 29200 -> ssthresh 14600 ->
cwnd 14600 + 3*1460 = 18980.

`newreno-rto`: FlightSize 29200 -> ssthresh 14600; cwnd 1460. The next single
ACK adds one MSS -> 2920.

`newreno-abc-partial-segment`: a 500-byte ACK in slow start adds 500, not
1460 -> 15100. This is the difference between ABC and per-ACK MSS counting.

## 3. CUBIC

CUBIC is the default algorithm. `beta = 7/10`, `C = 2/5`, and the
TCP-friendly slope is `alpha = 3*(1-beta)/(1+beta) = 9/17` (RFC 8312 §4.2,
§4.5). All three are exact rationals in the arithmetic below.

### 3.1 Loss event

Let `pre` be cwnd immediately before the loss and `w_last_max` the stored
previous maximum (initialised to 0).

```
if pre < w_last_max:                 # fast convergence, RFC 8312 §4.6
    w_max = floor(pre * 17 / 20)     # (1+beta)/2 = 17/20
else:
    w_max = pre
w_last_max = pre
ssthresh = max(2*MSS, floor(w_max * 7 / 10))
cwnd = ssthresh
w_max_segs = floor(w_max / MSS)
K_us = icbrt(7.5e17 * w_max_segs)   # Eq. 2, in microseconds
epoch = now
```

`w_max` is kept in bytes and is not forced to an MSS multiple; `w_max_segs` is
the floor used by the curve. `icbrt` is the integer floor cube root (binary
search on `u128`); no floating point is used. `K_us` is exact to the
microsecond floor.

The vector reports `expected_w_max` and `expected_k_us` so C13 can assert the
epoch state without recomputing it from the curve.

### 3.2 Growth at a sample time (one RTT boundary)

Let `t_us` be the elapsed time since the loss epoch, excluding idle periods
(RFC 8312 §5.8), and let `RTT_us` be the current smoothed RTT.

```
W_cubic_segs(t) = w_max_segs + trunc( C * (t - K)^3 )
W_est_segs(t)   = floor(7*w_max_segs/10)
                  + floor(9 * t_us / (17 * RTT_us))
target_segs     = max(2, W_cubic_segs, W_est_segs)
cwnd            = min(cwnd_max, target_segs * MSS)
```

`trunc` means toward zero for a negative `(t-K)`, so at `t = 0` the curve gives
`w_max_segs - C*K^3`, which is `beta*w_max` and equals `ssthresh` (up to the
integer floor). At `t = K` the curve reaches `w_max`; before `K` it is concave,
after `K` it is convex. When `W_est` is larger, CUBIC is in the TCP-friendly
region and adopts `W_est` (RFC 8312 §4.2).

The `advance` event is **one RTT of ACK-clocked growth**. The vector pins the
target at the marked time; an implementation that updates cwnd per ACK must
have reached the same value when the RTT ends. The intermediate values are not
pinned; a test may assert only that they monotonically approach the target and
never exceed it by more than one MSS.

### 3.3 Worked concave sequence

Initial cwnd 288 MSS (420480 bytes), loss:

```
w_max       = 420480                 (pre = 420480, w_last_max = 0)
w_max_segs  = 288
ssthresh    = floor(420480*7/10) = 294336
cwnd        = 294336
K_us        = icbrt(7.5e17 * 288) = 6,000,000 us (6 s)
```

At `t = 1 s`, `dt = -5 s`:

```
C*dt^3 = 0.4 * (-125) = -50
W_cubic = 288 - 50 = 238
W_est   = floor(0.7*288) + floor(9*1e6/(17*100000))
        = 201 + 5 = 206
target  = 238 -> cwnd = 238*1460 = 347480
```

At `t = 3 s`: `dt = -3`, `C*dt^3 = -10.8`, truncated toward zero `-10`, so
`W_cubic = 288 - 10 = 278`; `W_est = 201 + 15 = 216`, target 278 -> 405880.

At `t = 6 s`: `dt = 0`, `W_cubic = 288`, `W_est = 201 + 31 = 232`, target 288
-> 420480. This is the `K` plateau.

### 3.4 Worked convex sequence

At `t = 9 s`, `dt = 3`: `W_cubic = 288 + 0.4*27 = 298.8 -> 298`,
`W_est = 201 + 47 = 248`, target 298 -> 435080.

At `t = 12 s`, `dt = 6`: `W_cubic = 288 + 0.4*216 = 374.4 -> 374`,
`W_est = 201 + 63 = 264`, target 374 -> 546040.

### 3.5 Fast convergence

Initial `w_last_max = 1000 MSS` and `pre = 288 MSS`:

```
pre < w_last_max -> w_max = floor(288*17/20) = 244.8 -> 244 seg = 357408 bytes
ssthresh = floor(357408*7/10) = 250185
w_max_segs = 244, K_us = icbrt(7.5e17*244) = 5,677,411 us
```

The first vector row asserts 357408 / 250185 / 5677411. The later advance and
second loss show the reduced `w_last_max` affecting the next event.

### 3.6 Second loss and RTO

`cubic-second-loss`: after the first loss the connection grows to 405880 bytes
(277.9 MSS). A second loss at 405880 is below the stored `w_last_max = 420480`,
so fast convergence applies: `w_max = floor(405880*17/20) = 344998`,
`ssthresh = 241498`, `K_us = 5614672`.

`cubic-rto`: RFC 8312 §4.7 uses `ssthresh = beta*cwnd`, `cwnd = 1 MSS`,
`w_max = cwnd at the beginning of congestion avoidance`, `K = 0`. For
`cwnd = 420480`, ssthresh = 294336, cwnd = 1460, K = 0.

### 3.7 Idle

`cubic-idle-excluded`: the connection is idle for 6 s between 6 s and 12 s.
The curve is evaluated at effective `t = 6 s`, not 12 s, so the target stays
288 MSS. Without the exclusion it would be 374 MSS; that difference is the
point of the vector (RFC 8312 §5.8).

## 4. PRR (not an arithmetic vector)

PRR is the rate-halving mechanism used during recovery; it does not change
`cwnd` directly. RFC 6937 §3 is normative:

```
on entry:
  ssthresh      = target cwnd from the congestion algorithm
  prr_delivered = 0
  prr_out       = 0
  RecoverFS     = FlightSize at entry

on every ACK during recovery:
  DeliveredData = change in SND.UNA + change in SACKed bytes
  prr_delivered += DeliveredData
  pipe = RFC 6675 pipe
  if pipe > ssthresh:
      sndcnt = ceil(prr_delivered * ssthresh / RecoverFS) - prr_out
  else:
      limit  = max(prr_delivered - prr_out, DeliveredData) + MSS   # PRR-SSRB
      sndcnt = min(ssthresh - pipe, limit)
  send at most sndcnt bytes

on every transmission: prr_out += bytes_sent
```

The implementation MUST use PRR-SSRB, not the conservative CRB, because C13
requires a send rate near half rather than a stall. C13's behavioural test
asserts the in-flight curve; no separate PRR vector is required by C01 because
the formula is closed and depends on `pipe`, which `sack.toml` pins.

## 5. The `cwnd.toml` event schema

```toml
[[sequence]]
name = "..."
algorithm = "newreno" | "cubic"
mss = 1460
cwnd_max = <bytes>
initial_cwnd = <bytes>
initial_ssthresh = <bytes>
initial_w_last_max = <bytes>       # CUBIC only
rule = "RFC sections this sequence pins"
steps = [
  { event = "rtt", acks = N, expected_cwnd = ..., expected_ssthresh = ... },
  { event = "ack", acked = N, ... },
  { event = "loss", flight = N, ... },              # NewReno
  { event = "loss", expected_w_max = ..., ... },    # CUBIC
  { event = "partial_ack", acked = N, ... },
  { event = "full_ack", ... },
  { event = "rto", ... },
  { event = "idle_restart", ... },
  { event = "advance", at_ms = T, idle_ms = I, rtt_ms = R, ... },
]
```

Rules for replaying a sequence:

1. Start from `initial_cwnd`, `initial_ssthresh`, `cwnd_max`.
2. Replay steps in order; each `rtt` step is `acks` full-MSS new-data ACKs.
3. `loss` starts fast recovery exactly once; `partial_ack`/`full_ack` only
   apply while in recovery.
4. `rto` resets the loss window and the RTO-recovery flag as in §2.4.
5. CUBIC `advance` sets the epoch at `at_ms - idle_ms`; the next loss restarts
   the epoch.
6. Compare `cwnd` and `ssthresh` exactly (bytes). A mismatch is a failed
   vector, not a tolerance.

## 6. Review notes

* The NewReno per-ACK floor is visible in `newreno-ca-linear`: the increments
  are 146, 144, 142, ... and the sum over 10 ACKs is 1395, not 1460. A test
  that hardcodes +1 MSS per RTT will fail this vector, which is intentional.
* CUBIC's `W_est` uses the **current** smoothed RTT; the vectors use 100 ms.
  A connection with a different RTT will follow a different TCP-friendly
  slope; the curve `W_cubic` is RTT-independent by design.
* `K_us` is computed from `w_max_segs`, not from the byte-exact `w_max`. A
  parser of the vector must do the same floor division.
* No floating point appears anywhere in this specification or in the
  implementation. The cube root is an integer binary search; `C=2/5`,
  `beta=7/10` and `alpha=9/17` are exact rationals.
