<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SACK scoreboard and recovery arithmetic

Status: companion to `sack.toml` and normative for C12. It is the sender-side
scoreboard only. Receiver-side SACK/DSACK block generation is described in
§6; it shares the range representation but not the algorithms.

## 1. Representation and inputs

Sequence offsets are **bytes relative to `snd_una`**, so a vector is
independent of the absolute sequence space. `snd_una` is always 0 in the
vectors. The scoreboard is a sorted list of disjoint, half-open ranges
`[start, end)`:

| Field | Meaning |
|---|---|
| `sacked` | ranges cumulatively SACKed but not cumulatively ACKed; input state |
| `high_data` | `HighData - snd_una`, the highest byte sent plus one |
| `high_rxt` | `HighRxt - snd_una`, the highest byte retransmitted plus one; 0 means none |
| `rescue_rxt` | `RescueRxt - snd_una`, or -1 when undefined |
| `unsent_available` | there is new data beyond `HighData` and the receiver window allows it |
| `receiver_window` | `rwnd - snd_una`, the right edge of the send window |
| `range_cap` | maximum number of SACKed ranges the scoreboard may hold |
| `dsack` | a received DSACK block, as offsets relative to `snd_una` (may be negative) |
| `previously_sacked` | the scoreboard before this ACK, used to detect reneging |
| `dup_ack_threshold` | 3 in the vectors; the RFC's `DupThresh` |

A range is always clipped to `[0, high_data)` and merged with any overlapping
or adjacent range before it is stored. A SACK block outside the send window is
discarded and counted; it never grows the scoreboard.

## 2. `IsLost(S)`

RFC 6675 §4:

```
IsLost(S) = true if
    (number of discontiguous SACKed ranges above S) >= DupThresh
 or (SACKed bytes above S) > (DupThresh - 1) * SMSS
```

"Above S" means the part of each range with `end > S`; the byte count is
`sum(max(0, end - max(start, S)))`. The first test counts ranges whose `start`
is greater than S, not bytes. The byte test is strict `>`; a range of exactly
`(DupThresh-1)*SMSS` bytes above S is not enough. With `DupThresh=3` and
`MSS=1000`, a single SACKed range of 2500 bytes above S declares S lost, while
a 2000-byte range does not (`sack-bytes-threshold` vs `below-threshold`).

`S` is never inside a SACKed range when `IsLost` is called; the caller skips
those octets. `IsLost` is evaluated per byte in the definition, but a
range-based implementation may evaluate it per gap because the predicate is
constant within an unSACKed gap.

## 3. `SetPipe`

RFC 6675 §4, for every octet S in `[0, high_data)` that is not SACKed:

```
if not IsLost(S): pipe += 1
if S < high_rxt:  pipe += 1
```

The second condition counts a retransmitted octet a second time: once as the
original transmission and once as the retransmission. That is intentional and
is why `high-rxt-pipe` reports `pipe = 8000` for 5000 bytes of data with one
3000-byte hole and `high_rxt = 5000`: the two 2000-byte unSACKed regions are
counted twice, the SACKed 1000-byte region is not counted.

`pipe` is a byte count, not a segment count. It is used by PRR and by the
RFC 6675 `cwnd - pipe >= SMSS` send decision.

## 4. `NextSeg`

RFC 6675 §4, in order. `hs` is the highest octet covered by any SACK block.

```
(1) smallest unSACKed S2 with S2 > high_rxt, S2 < hs, IsLost(S2)
    -> after rule 1 must return one segment of up to SMSS starting at S2
(2) else if unsent data exists and the receiver window allows:
    -> one segment of up to SMSS of new data at high_data
(3) else smallest unSACKed S3 with S3 > high_rxt, S3 < hs (even if not lost)
    -> one segment of up to SMSS starting at S3
(4) else if high_ack > rescue_rxt (or rescue_rxt is undefined) and there is
    outstanding unSACKed data:
    -> one rescue segment including the highest outstanding unSACKed octet;
       set rescue_rxt = high_data (RecoveryPoint); do not change high_rxt
(5) else none
```

The vector's `next_seg.len` is the **gap length** capped at MSS: a retransmit
never includes already-SACKed bytes, even when the gap is shorter than an MSS.
`rule 2` returns `min(MSS, receiver_window - high_data)` bytes. A `none` result
is encoded as `start = -1, len = 0, kind = "none"`.

`high_rxt_after` is updated for rules 1 and 3 to `max(high_rxt, start+len)`;
rule 4 leaves it unchanged; rule 2 leaves it unchanged. `rescue_rxt_after` is
set to `high_data` only by rule 4.

## 5. Worked cases

### 5.1 `three-sacked-ranges`

```
sacked = [1000,2000), [3000,4000), [5000,6000), high_data = 6000
S = 0: sacked bytes above = 3000 > 2000 -> IsLost(0) = true
lost = [0,1000)
pipe: [0,1000) lost and not retransmitted -> 0
      [2000,3000) SACKed bytes above = 2000, ranges above = 2 -> not lost -> 1000
      [4000,5000) SACKed bytes above = 1000, ranges above = 1 -> not lost -> 1000
      pipe = 2000
NextSeg: rule 1 at S2 = 0 -> retransmit [0,1000)
```

### 5.2 `sack-bytes-threshold`

A single range `[1500,4000)` puts 2500 bytes above S=0, which is greater than
`2*MSS = 2000`; S=0 is lost. The whole `[0,1500)` gap is lost; pipe is 0
because it is lost and not yet retransmitted. NextSeg rule 1 returns
`[0,1000)`.

### 5.3 `below-threshold`

`[2000,3000)` is 1000 bytes above S=0, below the 2000-byte threshold, and only
one discontiguous range; S=0 is not lost. Pipe counts the whole 5000-byte
window minus the 1000 SACKed bytes = 4000. Rule 1 finds no lost S2, rule 2
sends new data at 5000.

### 5.4 `rule2-after-no-rule1`

`high_rxt = 1500` means rule 1 may only consider S2 > 1500. The gaps at 2000
and 4000 are not lost (2000 bytes above each, two ranges above at 2000 but
only one at 4000), so rule 1 fails. New data is available, so rule 2 wins even
though a lost range exists below HighRxt. This is the RFC's ordering: rule 1
is restricted to sequence numbers above HighRxt.

### 5.5 `rule3-no-unsent`

Same scoreboard, but no new data. Rule 3 returns the first unSACKed S3 above
HighRxt, even though it is not declared lost: `[2000,3000)`. This is the
"last resort" retransmission that keeps the ACK clock alive.

### 5.6 `rule4-rescue`

`sacked = [0,1000)`, `high_data = 3000`, no unsent data. Rule 1 and 3 fail
because the only unSACKed octets (1000..2999) are above the highest SACKed
octet (1000), so no S2/S3 is `< hs`. Rule 4 sends a rescue segment that
includes the highest outstanding octet 2999: start 2000, length 1000. It sets
`rescue_rxt = 3000` and does not change `high_rxt`.

### 5.7 `rule5-all-sacked`

`[0,3000)` covers all data, no unsent data; every rule fails; `none`.

### 5.8 `high-rxt-pipe`

`[2000,3000)` SACKed, `high_rxt = 5000`. For the unSACKed 0..1999: not lost
(1000 bytes above, one range), count 2000; `S < 5000`, count another 2000. For
3000..4999: not lost (no SACKed bytes above), count 2000; `S < 5000`, count
another 2000. Pipe = 8000. `NextSeg` rule 1 cannot start at S2 <= HighRxt;
there is no S2 > 5000, so rule 4 is reached and rescues [4000,5000).

### 5.9 DSACK

A DSACK block is a duplicate-SACK report: its right edge is at or below
`snd_una`, or it duplicates an already-SACKed range. It is advisory; the
scoreboard is not changed. `dsack-duplicate` has `dsack = [-1000,0)`, which is
entirely below `snd_una`; `dsack_detected = true`, `sacked_after` unchanged.
The sender uses the DSACK to detect a spurious retransmission (C14), not to
mark data lost.

### 5.10 Reneging

Reneging is the peer discarding data it previously SACKed. The vector supplies
`previously_sacked = [1000,3000)` and a new `sacked = []`; no cumulative ACK
covers 1000..3000. The scoreboard must rebuild conservatively: the missing
range is treated as unacknowledged, `reneging = true`, and `NextSeg` falls to
rule 4 because the only SACKed range is gone. The implementation MUST NOT
assume the old SACK remains valid; a lost retransmission of SACKed data is
worse than a duplicate.

### 5.11 Range cap

`cap-two` presents three ranges with `range_cap = 2`. The new third block is
**dropped**, not merged across the gap (merging would mark unsent octets as
SACKed, which is optimistic). `dropped = 1`; `sacked_after` keeps the first
two ranges; `NextSeg` rule 3 chooses [0,1000) because no range is declared
lost. A flood of tiny SACK blocks can therefore cost information, never
memory.

## 6. Receiver-side SACK and DSACK (C12's other half)

The receiver generates blocks from its reassembly state, not from this
sender scoreboard:

* `SACK-permitted` is offered on SYN and SYN-ACK when `tcp.sack_enable`.
* The first SACK block in an ACK MUST cover the segment that triggered the
  ACK if that segment advanced the cumulative ACK; otherwise the receiver
  reports the lowest-numbered hole first (RFC 2018 §3).
* The number of blocks is limited by the option space: with timestamps the
  data segment has 40 - 12 = 28 bytes of option space, which fits three
  8-byte blocks plus the 2-byte kind/length and padding; without timestamps,
  four blocks fit. The receiver MUST NOT emit a block that does not fit; it
  drops the least useful block and counts `tcp.sack_blocks_dropped`.
* DSACK (RFC 2883): a duplicate segment is reported with a block whose left
  edge is the duplicate range and whose right edge is at or below the
  cumulative ACK. The first block in a DSACK ACK carries the duplicate; a
  second block may carry the normal SACK state.
* The receiver never removes SACKed data from its reassembly buffer until it
  is cumulatively acknowledged (RFC 2018); reneging is therefore the peer's
  choice, not ours.

## 7. Edge conditions the vectors pin

* `high_rxt` is exclusive: a byte at offset `high_rxt` has not been
  retransmitted. Rule 1 requires `S2 > high_rxt`, so a retransmission at
  `high_rxt` is not selected again.
* `Pipe` counts a retransmitted octet twice only while it is not SACKed; once
  it is SACKed, it is removed from both counts.
* A rescue retransmission is allowed once per recovery entry; `rescue_rxt`
  remains set to `RecoveryPoint` (high_data at entry) and rule 4 fails on the
  next call (`rule4-existing-rescue`).
* An ACK that advances `snd_una` trims every range below the new `snd_una`;
  the vector format uses relative offsets and does not include a cumulative
  ACK event except in the reneging case.
* The scoreboard never stores a range that extends beyond `high_data`; a SACK
  block beyond `high_data` is a protocol error and is dropped.

## 8. Implementation checklist for C12

1. Store ranges, not bytes; merge on insert; clip to the send window.
2. Implement `IsLost` exactly as §2; do not substitute a simple "gap length >
   2 MSS" test, because the discontiguous-range clause differs.
3. Implement `SetPipe` with the double count for `S < high_rxt`.
4. Implement all five `NextSeg` rules in order; rule 2 is not a fallback for
   rule 1.
5. Never let the range count exceed `tcp.sack_ranges_max`; drop the new block
   and count.
6. On RTO, keep or expunge SACK state per RFC 2018/6675 §5.1; the vector set
   does not test that choice. Our choice: expunge, because a timeout may mean
   the receiver reneged; the connection recovers from cumulative ACK state.
7. On reneging, rebuild from the new SACK information and count; never
   resurrect a range that the new ACK no longer covers.
8. Receiver: generate blocks in RFC 2018 order, limit by actual option space,
   and emit DSACK for duplicates.
