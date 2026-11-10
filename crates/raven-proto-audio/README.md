<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-proto-audio

Wire contract between the `audio:` scheme server (`audiod`), its clients and sound drivers. It
exists so the mixer, the Intel HDA driver and the Pi 4 PWM driver can be written in parallel
against one versioned layout (`tickets/next-backlogs/001-next-backlog-hw-complete.md` section 5.3).

The crate is `no_std`, has one dependency (`raven-abi` for errno and handle types), forbids
`unsafe`, never allocates in an encoder, never uses floating point and never panics while
decoding. Every table below is pinned by a golden test in `tests/golden.rs` or in the module
tests.

## Rules

* All integers are **little-endian** and packed explicitly; there is no implicit padding.
* Every message starts with `u16 version = 1`. A different version is rejected with
  `DecodeError::UnsupportedVersion`.
* Bounds: `audio:` path 32 bytes, `WRITE` payload 65 536 bytes, rate `8000..=192000`, channels
  `1..=8`, period frames `1..=1048576`, periods `1..=256`, ring region at most 64 MiB.
* Handles never travel as pointers. `OPEN` attaches the stream handle through the kernel; for
  `SET_BUFFER` the client first calls `HANDLE_TRANSFER` with the memory object and puts the
  **destination-valid** handle value in the message.
* Errno values are `raven-abi` numbers (Linux values, DR-13). The audio protocol's `ENOTSUP` is
  the ABI's `EOPNOTSUPP` (95).
* Decoders consume exactly the whole input; trailing bytes are an error.

## Stream requests

Common header:

| Offset | Size | Field |
|---|---|---|
| 0 | 2 | `version` = 1 |
| 2 | 2 | `kind` |

Kinds: `1` OPEN, `2` WRITE, `3` CALL.

| Kind | Body after offset 4 |
|---|---|
| OPEN | `path_len: u16`, then `path_len` bytes of UTF-8 `audio:out<N>` (`1..=32`) |
| WRITE | `len: u32`, then `len` PCM bytes (`0..=65536`) |
| CALL | `op: u16`, then the operation arguments below |

Example OPEN of `audio:out0`:

```text
01 00 01 00 0a 00 61 75 64 69 6f 3a 6f 75 74 30
version  kind  len  a  u  d  i  o  :  o  u  t  0
```

Example WRITE of four bytes `01 02 03 04`:

```text
01 00 02 00 04 00 00 00 01 02 03 04
version  kind  len           pcm
```

### CALL operations

| Op | Name | Arguments after `op: u16` |
|---|---|---|
| 1 | SET_FORMAT | format record (6 bytes) |
| 2 | SET_BUFFER | `handle: u32`, `period_frames: u32`, `period_count: u16` |
| 3 | START | none |
| 4 | STOP | none |
| 5 | DRAIN | none |
| 6 | SET_VOLUME | `left: u16`, `right: u16` (`0..=65535` each) |
| 7 | GET_STATUS | none |

Format record (6 bytes): `rate: u32`, `channels: u8`, `sample_format: u8`.

Sample format codes:

| Code | Name | Bytes/sample | Status |
|---|---|---|---|
| 1 | S16LE | 2 | implemented |
| 2 | S24LE | 3 | reserved, `ENOTSUP` |
| 3 | S32LE | 4 | reserved, `ENOTSUP` |
| 4 | F32LE | 4 | reserved, `ENOTSUP` |

Example SET_FORMAT 48 kHz stereo S16LE:

```text
01 00 03 00 01 00 80 bb 00 00 02 01
version  CALL  SET_FORMAT rate=48000 ch=2 S16LE
```

Reserved formats are well-formed on the wire: the decoder accepts them and `Call::check()`
returns `ENOTSUP`, so the client receives the documented error instead of a framing failure. An
unknown code, an out-of-range rate or zero/too many channels is rejected as a malformed request.

Example SET_BUFFER with handle 7, 1024 frames per period and 4 periods:

```text
01 00 03 00 02 00 07 00 00 00 00 04 00 00 04 00
version  CALL  SET_BUFFER handle  period=1024  count=4
```

`SET_BUFFER` attachment: the client creates a memory object of at least
`align_up(80, 64) + period_frames * period_count * frame_bytes` bytes, calls
`HANDLE_TRANSFER(audiod_process, object, MAP|READ|WRITE)` and sends the returned
destination-valid handle in this message. `audiod` maps it, initialises the header, validates the
declared size and answers with the granted geometry. Plain `WRITE` is the simple copy path and
needs no shared memory.

## Stream responses

| Offset | Size | Field |
|---|---|---|
| 0 | 2 | `version` = 1 |
| 2 | 2 | `status`: 0 on success, otherwise the errno |
| 4 | 2 | `tag` |

| Tag | Body |
|---|---|
| 0 OK | none |
| 1 STREAM | `id: u32` |
| 2 BUFFER | `period_frames: u32`, `period_count: u16`, `region_len: u32` |
| 3 STATUS | `position_frames: u64`, `underruns: u32`, `state: u8` |
| 4 ERR | none; `status` carries the errno and must be non-zero |

States: `0` stopped, `1` running, `2` draining, `3` underrun.

Example STATUS after 1000 frames with 2 underruns while running:

```text
01 00 00 00 03 00 e8 03 00 00 00 00 00 00 02 00 00 00 01
version  ok  STATUS  position=1000      underruns=2     running
```

`ENOTSUP` is `01 00 5f 00 04 00`. A tag other than ERR with a non-zero status, or ERR with
status zero, is malformed.

## Driver ring

`audiod` and a driver share one region created by the kernel (`dma_alloc`, S21b). The first 80
bytes are the header, the PCM area starts at the next 64-byte boundary (offset 128):

| Offset | Size | Field | Writer |
|---|---|---|---|
| 0 | 8 | magic `RAVENAUD` | creator |
| 8 | 2 | version = 1 | creator |
| 10 | 2 | header length = 80 | creator |
| 12 | 4 | `capacity_frames` | creator |
| 16 | 4 | `period_frames` | creator |
| 20 | 2 | `period_count` | creator |
| 22 | 2 | `frame_bytes` | creator |
| 24 | 4 | `rate` | creator |
| 28 | 1 | `channels` | creator |
| 29 | 1 | `sample_format` | creator |
| 30 | 1 | `state` | audiod |
| 31 | 1 | reserved = 0 | creator |
| 32 | 8 | `read_index` (frames) | driver |
| 40 | 8 | `write_index` (frames) | audiod |
| 48 | 8 | `start_index` (frames) | audiod |
| 56 | 4 | `underruns` | driver |
| 60 | 4 | reserved = 0 | creator |
| 64 | 8 | `doorbell` (S26 user event id) | creator |
| 72 | 4 | `data_offset` = 128 | creator |
| 76 | 4 | `data_len` | creator |

Layout and bounds:

```text
capacity      = period_frames * period_count                  (1..=1_048_576 frames)
frame_bytes   = channels * bytes_per_sample                   (S16LE: channels * 2)
data_offset   = align_up(80, 64) = 128
data_len      = capacity * frame_bytes
region_len    = data_offset + data_len                        (exact; at most 64 MiB)
frame i       at data_offset + (i % capacity) * frame_bytes
```

`period_count <= 256`, `period_frames > 0`, the format must be implemented (`S16LE`), and the
region size must be exact; a header with a bad magic, version, format record, capacity, reserved
field or index relation is rejected by `RingHeader::read_from`.

Indices count frames, not bytes, and are monotonic `u32`-valued `u64` counters. `available =
write_index - read_index` (clamped to capacity), free = capacity - available. Slot =
index % capacity. A short write or read is normal and returns the frame count moved.

### Position and period semantics

* `position_frames = read_index - start_index`. `START` snapshots `start_index = read_index`,
  so position is zero at the start of each play interval. `STOP` freezes it; the next `START`
  begins a new interval at zero.
* `periods_played = read_index / period_frames` and `periods_written =
  write_index / period_frames` are derived counters; they do not reset when the ring wraps.
* A consumer that asks for one period and finds fewer than `period_frames` available reports
  `RingError::Underrun`, consumes nothing and increments the header's `underruns` counter by
  one. The driver plays silence for that period. `audiod` observes the counter and may set
  `state = 3` (underrun).
* The attached client ring uses the same header and helpers with the roles swapped: the client
  is the producer (`write_index`) and `audiod` the consumer (`read_index`); `state`,
  `start_index` and the device position fields are not used on that ring.

### Memory ordering

Single producer, single consumer, one writer per field:

1. **Producer**: write PCM frames, `fence(Release)`, store `write_index`, signal the doorbell.
   The release fence publishes the samples before the index.
2. **Consumer**: `fence(Acquire)`, read `write_index`, read the frames, `fence(Release)`, store
   `read_index`. The acquire fence prevents reading the index before the samples; the release
   fence retires the reads before the slot is reused.
3. `state` is written only by `audiod`; `underruns` only by the driver. The doorbell is one S26
   user event signalled once per batch, not per frame.

`RingProducer`/`RingConsumer` implement this contract over `&mut [u8]`. Because the crate
forbids `unsafe`, indices are plain byte loads/stores with `fence`; a component that needs Rust
atomics must provide the same ordering around the same bytes. The host test uses a mutex and a
single process; the real users are separate processes with separate mappings.

## Paths

`audio:out<N>` where `N` is `0..=31` in decimal without leading zeros. `out0` is the default
output device. Anything else is a `PathError`; `OPEN` validates the path before accepting it.

## Tests

```text
cargo test -p raven-proto-audio
cargo clippy -p raven-proto-audio --all-targets -- -D warnings
cargo build -p raven-proto-audio --target x86_64-unknown-none --release
```

The suite covers golden bytes for every message family, round-trips, truncation, trailing bytes,
out-of-range formats, reserved-format `ENOTSUP`, 4,000 random inputs per decoder, ring layout and
header validation, wrap-around, underrun accounting, period accounting, position semantics and a
two-thread producer/consumer run.
