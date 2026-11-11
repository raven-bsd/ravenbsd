# RTO arithmetic

Status: companion to `rto.toml` and normative for C10. The estimator is the
RFC 6298 algorithm in integer arithmetic. No floating point is used anywhere in
the stack, and the vector values are exact.

## 1. Representation

* `SRTT` and `RTTVAR` are kept in **q8**: eighths of a millisecond. One
  millisecond is 8 units. The values are `u64`; they cannot overflow for any
  RTT the stack accepts (a sample is clamped to 24 hours before addition).
* An RTT sample is `R` whole milliseconds (the C04 simulator and the timestamp
  path both produce integer milliseconds; sub-millisecond RTT rounds down to 0).
  In q8, `Rq = 8*R`.
* `G` is the clock granularity, 1 ms by default (`clock_granularity_ms` in the
  vector), so `Gq = 8`.
* `K = 4` (RFC 6298 §2.3).
* The timer value is whole milliseconds. `rto_computed_ms` is the formula's
  result before the configured clamp; `rto_ms` is the value actually armed.

## 2. Algorithm

On the **first** sample `R`:

```
SRTT_q8    = 8*R
RTTVAR_q8  = 4*R              (R/2 in q8)
```

On every **subsequent** sample `R`:

```
diff_q8    = abs(SRTT_q8 - 8*R)
RTTVAR_q8  = ceil( (3*RTTVAR_q8 + diff_q8) / 4 )
SRTT_q8    = ceil( (7*SRTT_q8  + 8*R)      / 8 )
```

Both divisions round **up**. `ceil` is chosen so the estimator never
under-estimates the variance or the mean on a fractional millisecond; it is
deterministic and easy to reproduce. A reviewer can check any row with integer
arithmetic only.

After every sample, including the first:

```
margin_q8     = max(Gq, 4*RTTVAR_q8)
rto_computed  = ceil( (SRTT_q8 + margin_q8) / 8 )
rto           = min(max(rto_computed, rto_min_ms), rto_max_ms)
```

On a retransmission timeout:

```
rto = min(2*rto, rto_max_ms)
```

`SRTT` and `RTTVAR` do not change on a timeout. A retransmitted segment is
never used for an RTT sample (Karn's algorithm): the sample clock for the
oldest unacknowledged segment is discarded when it is retransmitted. A later
segment that was never retransmitted may produce a sample. The vector
timeline's `timeout` steps exercise the backoff alone; C10's tests pair them
with a retransmitted segment to prove Karn.

The first RTO before any sample is `tcp.rto_initial` (1000 ms by default). A
timeout before the first sample doubles it to 2000 ms; `timeout-before-any-sample`
pins that.

## 3. Clamping and the RFC 1-second floor

RFC 6298 §2.4 says an RTO below 1 second **SHOULD** be rounded up to 1 second.
RavenBSD uses the configured `tcp.rto_min` instead, default **200 ms**. This is
a deliberate deviation for low-latency paths and for the C10 requirement that
the floor be configurable:

* `DEVIATION-06` (also called the RTO-floor deviation): `tcp.rto_min`
  defaults to 200 ms, not the RFC's 1 s SHOULD. Setting `tcp.rto_min = 1000`
  restores the RFC default exactly.
  Reason: the stack runs on loopback, virtual links and a 5G/latency-sensitive
  future; a 1 s floor on a 1 ms path wastes a second on every loss. The
  estimator's arithmetic is unchanged, and the clamp is visible in the
  property table, so an operator can choose the conservative value.
* The upper clamp is `tcp.rto_max`, default 60000 ms. Exponential backoff is
  capped there; it does not wrap or saturate to infinity.

The vector files set `rto_min_ms = 1` for the arithmetic sequences (so the
formula is visible), `100` for `min-clamp-100`, and `200` for
`min-default-200`. C10 sets the property to the vector's value before replaying
the sequence.

## 4. Worked sequences

### 4.1 `single-100`

```
R          = 100
SRTT_q8    = 8*100 = 800          (100.000 ms)
RTTVAR_q8  = 4*100 = 400          (50.000 ms)
margin_q8  = max(8, 4*400) = 1600 (200.000 ms)
RTO        = ceil((800+1600)/8) = 300 ms
```

### 4.2 `two-100-200`

First sample as above. Second sample `R = 200`:

```
Rq          = 1600
diff_q8     = |800 - 1600| = 800
RTTVAR_q8   = ceil((3*400 + 800)/4) = ceil(2000/4) = 500   (62.500 ms)
SRTT_q8     = ceil((7*800 + 1600)/8) = ceil(7200/8) = 900  (112.500 ms)
margin_q8   = max(8, 4*500) = 2000                          (250.000 ms)
RTO         = ceil((900+2000)/8) = ceil(362.5) = 363 ms
```

### 4.3 `increasing`, third sample

After `10` and `20`: `SRTT_q8 = 90`, `RTTVAR_q8 = 50`. Sample `R = 40`:

```
Rq          = 320
diff_q8     = |90 - 320| = 230
RTTVAR_q8   = ceil((3*50 + 230)/4) = ceil(380/4) = 95
SRTT_q8     = ceil((7*90 + 320)/8) = ceil(950/8) = ceil(118.75) = 119
margin_q8   = max(8, 4*95) = 380
RTO         = ceil((119+380)/8) = ceil(62.375) = 63 ms
```

### 4.4 `fractional-rounding`, second sample

First sample `7`: `SRTT_q8 = 56`, `RTTVAR_q8 = 28`. Sample `R = 11`:

```
diff_q8     = |56 - 88| = 32
RTTVAR_q8   = ceil((3*28 + 32)/4) = ceil(116/4) = 29
SRTT_q8     = ceil((7*56 + 88)/8) = ceil(480/8) = 60
margin_q8   = max(8, 4*29) = 116
RTO         = ceil((60+116)/8) = ceil(22) = 22 ms
```

The `ceil` at each smoothing step is what makes the third sample `25`, not
`24`; the vector pins it.

### 4.5 `min-clamp-100`

Sample `1` gives a computed RTO of 3 ms, but `rto_min_ms = 100`, so the armed
RTO is 100. The second sample changes `RTTVAR_q8` from 4 to 3, still computes
3, and remains clamped. This proves that the clamp is applied after the
formula and that `rto_computed_ms` stays visible for diagnosis.

### 4.6 `backoff-100`

After the first sample the RTO is 300. Three timeouts produce:

```
300 -> 600 -> 1200 -> 2400
```

`SRTT_q8` and `RTTVAR_q8` stay at 800/400; only the timer doubles.

### 4.7 `backoff-to-cap`

`R = 3000` gives `SRTT_q8 = 24000`, `RTTVAR_q8 = 12000`, RTO 9000. Timeouts:
9000 -> 18000 -> 36000 -> 60000 (cap) -> 60000. The cap is applied to the
doubled value, so the timer never exceeds `rto_max_ms`.

## 5. What the vectors do not cover

* Karn's algorithm and the sample-clock lifetime are tested in C10 with a
  retransmission event, not by an RTO arithmetic sequence. The vector timeline
  has no notion of which segment was retransmitted.
* Timestamp-derived RTT samples are C11's; they feed the same estimator and
  must reproduce these vectors when the sample is the same.
* RTO aging/decay after an idle period is not specified by RFC 6298; this
  spec leaves it `UNSPECIFIED` and does not age the timer. C24 measures the
  reference and records the difference if one is visible.
* The initial RTO is a property, not a sample; `tcp.rto_initial` is read
  before the first sample and is not changed by the vectors.

## 6. Implementation checklist for C10

1. Keep `SRTT_q8` and `RTTVAR_q8` as integers; never divide a millisecond
   into a float.
2. Use the exact `ceil` divisions above.
3. Apply the margin with `Gq` (8) and `K=4`.
4. Clamp after the formula, then double on timeout and clamp again.
5. Discard the sample clock on retransmission (Karn).
6. Recompute the RTO from the estimator when a fresh sample arrives, even if
   the timer had backed off (the `backoff-then-sample` sequence).
7. Expose `srtt_ms`, `rttvar_ms` and `rto_ms` in `SocketInfo`; the first two
   are `ceil(q8/8)` and the last is the armed value.
