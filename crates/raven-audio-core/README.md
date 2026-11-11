<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-audio-core

Integer-only audio math for RavenBSD: PCM format conversion, a saturating mixer, click-free
volume ramps, resampling, ring-buffer arithmetic, a strict WAV reader/writer and test tones. It
is the shared core that `audiod`, the Intel HDA and Pi 4 PWM drivers and the `play` tool reuse
(`tickets/next-backlogs/001-next-backlog-hw-complete.md` section 5.3, B53).

The crate is `no_std` + `alloc`, has no dependencies, forbids `unsafe`, never uses floating
point (DR-14), does no I/O and is deterministic: the same input produces the same bytes on every
run. Input from files and devices is validated and returned as a typed error or `None`; nothing
panics on outside data.

## Modules

| Module | Contents |
|---|---|
| `format` | `Format { rate, channels }` with 8 000..=192 000 Hz and 1..=8 channels; mono/stereo/upmix/downmix and a generic `convert` |
| `mix` | `mix(&[Stream], &mut [i16])` with i64 accumulation, saturation and Q15 per-stream gain |
| `ramp` | `ramp` / `envelope`: a linear gain ramp over the frames of a buffer |
| `resample` | `resample_linear`, `SincResampler` (polyphase windowed sinc), `output_frames` |
| `ring` | frames <-> bytes, whole/partial frames, contiguous runs, wrap advance, free/used bytes |
| `wav` | strict RIFF PCM16 `parse` and canonical `encode` |
| `tone` | `Tone` with `Sine` (1024-entry Q15 table), `Square` and `Silence` |

## Scales

* Samples are interleaved signed 16-bit PCM, `-32768..=32767`.
* Gains and amplitudes are Q15: `32768` is 1.0, `16384` is 0.5. `mix` clamps larger gains to
  `GAIN_UNITY`; the ramp and the tone generator do the same for gain and amplitude input.
* The mixer accumulates in `i64`, so any number of full-scale streams cannot overflow the
  intermediate sum, and the saturated result is independent of the stream order.

## Resampling

Both resamplers obey `out = round(in * to / from)` with halves rounded up. `resample_linear` is
one multiply and one shift per sample. `SincResampler` uses a Blackman-windowed sinc with 65 taps
and 256 phase rows, blended linearly between neighbouring rows, so the phase error is second
order instead of 1/256 of a sample. When the output rate is lower, the kernel cutoff is scaled to
`to / from` so the conversion band-limits instead of aliasing. The coefficient rows are
normalised to a DC gain of exactly 32768/32768.

`SINC_MAX_ERROR_LSB` is the documented bound: **200 LSB for a 0.5 full-scale sine from 100 Hz to
15 kHz, in both directions between 44 100 Hz and 48 000 Hz**, measured with the first and last 64
output frames excluded (the zero-padded filter window is still settling there). The observed
worst case is 173 LSB:

<!-- Measured on the host with the public API; amplitude 16384 (0.5 FS). -->
| Frequency | 44.1 -> 48 kHz | 48 -> 44.1 kHz |
|---|---|---|
| 100 Hz | 6 | 5 |
| 440 Hz | 22 | 19 |
| 1 kHz | 29 | 29 |
| 2 kHz | 13 | 5 |
| 3 kHz | 24 | 32 |
| 5 kHz | 32 | 42 |
| 8 kHz | 67 | 21 |
| 10 kHz | 69 | 36 |
| 12 kHz | 30 | 29 |
| 15 kHz | 76 | 173 |

For comparison, linear interpolation of the same 8 kHz tone errs by roughly 2 600 LSB. Above
15 kHz the error grows towards the transition band; `resample_linear` is the cheap path and
`SincResampler` the quality path.

A same-rate pass through `SincResampler` differs from the input by at most one LSB, because Q15
cannot store a unity tap of 32768 and the single residual unit sits on the next tap.

## WAV

`wav::parse` accepts uncompressed PCM (tag 1), 16-bit, 1..=8 channels, 8 000..=192 000 Hz. It
handles `fmt ` and `data` in either order, unknown chunks before or after them, odd-sized chunks
with their RIFF pad byte, and `fmt ` sizes of 16, 18 or 40 bytes. It rejects, without panicking,
a chunk that runs past the buffer, a partial trailing frame, an odd or unknown `fmt ` size, a
reserved format tag or sample width, and an out-of-range rate or channel count.
`wav::encode` writes the canonical 44-byte header and returns `None` for anything `parse` would
reject, so `parse(encode(wav)) == wav` for every accepted file.

## Tests

`cargo test -p raven-audio-core` runs 92 host tests, including a 10 000-iteration random-mix
property test (no overflow, order independent), 10 000 random length-rule checks, 500 random WAV
round trips, 4 000 random byte strings and 4 000 mutations of a valid WAV for the parser, and the
resampler error bound at ten frequencies in both directions.

## Provenance

Fresh implementation; no donor code was copied. The RIFF chunk layout is used as specified by
the format, the sine comes from a truncated Taylor series in integer arithmetic, and the
resampler is a standard windowed-sinc polyphase filter.
