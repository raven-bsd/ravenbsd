// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RIFF/WAVE parsing and signal checks for the WAV oracle.
//!
//! Only uncompressed PCM16 mono/stereo is meaningful for `-audiodev wav`; other encodings are
//! rejected with a reason. QEMU may leave the `data` chunk size as 0 or `0xFFFFFFFF` until the
//! file is closed, so those values mean "use the rest of the file".

use anyhow::{Context, Result, bail};

use super::{OracleCheck, OracleSpec};

/// A parsed PCM16 WAVE file with interleaved samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wav {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Channel count: 1 or 2.
    pub channels: u16,
    /// Interleaved signed 16-bit samples.
    pub samples: Vec<i16>,
}

impl Wav {
    /// Number of sample frames (samples per channel).
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / usize::from(self.channels)
        }
    }

    /// Duration in milliseconds.
    pub fn duration_ms(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.frames() as f64 * 1000.0 / f64::from(self.sample_rate)
        }
    }
}

struct Fmt {
    channels: u16,
    sample_rate: u32,
    block_align: u16,
}

/// Parse a PCM16 RIFF/WAVE file.
pub fn parse_wav(bytes: &[u8]) -> Result<Wav> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        bail!("not a RIFF/WAVE file");
    }
    let mut position = 12usize;
    let mut fmt: Option<Fmt> = None;
    let mut data: Option<(usize, usize)> = None;
    while position + 8 <= bytes.len() {
        let id = &bytes[position..position + 4];
        let declared = read_u32(bytes, position + 4).context("chunk size")? as usize;
        let body = position + 8;
        let available = bytes.len() - body;
        let actual = if id == b"data" && (declared == 0 || declared == u32::MAX as usize) {
            available
        } else {
            declared
        };
        if actual > available {
            bail!(
                "truncated {} chunk: declares {} byte(s), {} available",
                printable_id(id),
                actual,
                available
            );
        }
        match id {
            b"fmt " => fmt = Some(parse_fmt(&bytes[body..body + actual])?),
            b"data" if data.is_none() => data = Some((body, actual)),
            _ => {}
        }
        let padded = actual.checked_add(actual & 1).context("chunk size")?;
        position = body.checked_add(padded).context("chunk offset")?;
    }
    let fmt = fmt.context("missing fmt chunk")?;
    let (data_start, data_len) = data.context("missing data chunk")?;
    let block_align = usize::from(fmt.block_align);
    if block_align == 0 {
        bail!("fmt chunk has a zero block alignment");
    }
    let frames = data_len / block_align;
    let sample_count = frames
        .checked_mul(usize::from(fmt.channels))
        .context("sample count overflow")?;
    let mut samples = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        let offset = data_start + index * 2;
        let bytes = bytes
            .get(offset..offset + 2)
            .context("truncated PCM sample")?;
        samples.push(i16::from_le_bytes([bytes[0], bytes[1]]));
    }
    Ok(Wav {
        sample_rate: fmt.sample_rate,
        channels: fmt.channels,
        samples,
    })
}

fn parse_fmt(bytes: &[u8]) -> Result<Fmt> {
    if bytes.len() < 16 {
        bail!("fmt chunk is too short");
    }
    let format = read_u16(bytes, 0).context("fmt format")?;
    let channels = read_u16(bytes, 2).context("fmt channels")?;
    let sample_rate = read_u32(bytes, 4).context("fmt sample rate")?;
    let block_align = read_u16(bytes, 12).context("fmt block alignment")?;
    let bits = read_u16(bytes, 14).context("fmt bits")?;
    if format != 1 {
        bail!("unsupported WAVE format {format}; only PCM (1) is accepted");
    }
    if !matches!(channels, 1 | 2) {
        bail!("unsupported channel count {channels}; only mono/stereo are accepted");
    }
    if bits != 16 {
        bail!("unsupported sample width {bits}; only PCM16 is accepted");
    }
    if sample_rate == 0 {
        bail!("sample rate must be greater than zero");
    }
    if block_align != channels.saturating_mul(2) {
        bail!(
            "block alignment {} does not match {channels} channel(s) of 16-bit samples",
            block_align
        );
    }
    Ok(Fmt {
        channels,
        sample_rate,
        block_align,
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn printable_id(id: &[u8]) -> String {
    id.iter()
        .map(|byte| {
            if byte.is_ascii_graphic() {
                char::from(*byte)
            } else {
                '?'
            }
        })
        .collect()
}

/// Run the checks requested by `spec` against a parsed file.
pub(super) fn analyze(wav: &Wav, spec: &OracleSpec) -> Vec<OracleCheck> {
    let mut checks = Vec::new();
    if let Some(minimum) = spec.min_duration_ms {
        let duration = wav.duration_ms();
        let passed = duration >= minimum as f64;
        checks.push(if passed {
            OracleCheck::pass(
                "min_duration_ms",
                format!("{duration:.1} ms >= {minimum} ms"),
            )
        } else {
            OracleCheck::fail(
                "min_duration_ms",
                format!("{duration:.1} ms < {minimum} ms"),
            )
        });
    }
    if let Some(minimum) = spec.min_rms {
        let rms = rms(wav);
        let passed = rms >= minimum;
        checks.push(if passed {
            OracleCheck::pass("min_rms", format!("{rms:.6} >= {minimum:.6}"))
        } else {
            OracleCheck::fail("min_rms", format!("{rms:.6} < {minimum:.6}"))
        });
    }
    if let Some(target) = spec.tone_hz {
        let tolerance = spec.tolerance_percent.unwrap_or(0.0);
        match dominant_frequency(wav) {
            Some(found) => {
                let error = (found - target).abs();
                let allowed = target * tolerance / 100.0;
                let passed = error <= allowed;
                checks.push(if passed {
                    OracleCheck::pass(
                        "tone_hz",
                        format!(
                            "dominant {found:.2} Hz is within {tolerance:.2}% of {target:.2} Hz"
                        ),
                    )
                } else {
                    OracleCheck::fail(
                        "tone_hz",
                        format!(
                            "dominant {found:.2} Hz is {error:.2} Hz from {target:.2} Hz \
                             (allowed {allowed:.2} Hz, {tolerance:.2}%)"
                        ),
                    )
                });
            }
            None => checks.push(OracleCheck::fail(
                "tone_hz",
                format!("no dominant frequency found; expected {target:.2} Hz"),
            )),
        }
    }
    if let Some(maximum) = spec.max_gap_ms {
        let (gap_ms, has_signal) = longest_zero_gap_ms(wav);
        let passed = !has_signal || gap_ms <= maximum as f64;
        checks.push(if passed {
            OracleCheck::pass(
                "max_gap_ms",
                if has_signal {
                    format!("longest zero gap {gap_ms:.1} ms <= {maximum} ms")
                } else {
                    "no non-zero sample; gap check not applicable".to_string()
                },
            )
        } else {
            OracleCheck::fail(
                "max_gap_ms",
                format!("longest zero gap {gap_ms:.1} ms > {maximum} ms"),
            )
        });
    }
    if checks.is_empty() {
        checks.push(OracleCheck::pass(
            "file",
            "valid PCM16 RIFF/WAVE file".to_string(),
        ));
    }
    checks
}

fn rms(wav: &Wav) -> f64 {
    if wav.samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = wav
        .samples
        .iter()
        .map(|sample| {
            let value = f64::from(*sample) / 32768.0;
            value * value
        })
        .sum();
    (sum / wav.samples.len() as f64).sqrt()
}

fn longest_zero_gap_ms(wav: &Wav) -> (f64, bool) {
    let channels = usize::from(wav.channels.max(1));
    let frames = wav.frames();
    let mut started = false;
    let mut run = 0usize;
    let mut longest = 0usize;
    for frame in 0..frames {
        let base = frame * channels;
        let silent = wav.samples[base..base + channels]
            .iter()
            .all(|sample| *sample == 0);
        if !started {
            if silent {
                continue;
            }
            started = true;
            continue;
        }
        if silent {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let rate = f64::from(wav.sample_rate.max(1));
    (longest as f64 * 1000.0 / rate, started)
}

/// Estimate the dominant frequency with a Hann-windowed radix-2 FFT and parabolic interpolation.
fn dominant_frequency(wav: &Wav) -> Option<f64> {
    let channels = usize::from(wav.channels.max(1));
    let frames = wav.frames();
    let start = (0..frames).find(|frame| {
        let base = frame * channels;
        wav.samples[base..base + channels]
            .iter()
            .any(|sample| *sample != 0)
    })?;
    let available = frames - start;
    if available < 64 {
        return None;
    }
    let target = available.min(32_768);
    let size = target.next_power_of_two().clamp(1024, 32_768);
    let used = available.min(size);
    if used < 64 {
        return None;
    }
    let mut mono = Vec::with_capacity(used);
    for frame in 0..used {
        let base = (start + frame) * channels;
        let sum: i64 = wav.samples[base..base + channels]
            .iter()
            .map(|sample| i64::from(*sample))
            .sum();
        mono.push(sum as f64 / f64::from(wav.channels));
    }
    let mean = mono.iter().sum::<f64>() / mono.len() as f64;
    let mut buffer = vec![Complex::default(); size];
    for (index, value) in mono.iter().enumerate() {
        let window = if used > 1 {
            0.5 * (1.0 - (std::f64::consts::TAU * index as f64 / (used - 1) as f64).cos())
        } else {
            1.0
        };
        buffer[index] = Complex {
            re: (value - mean) * window,
            im: 0.0,
        };
    }
    fft(&mut buffer);
    let half = size / 2;
    if half < 2 {
        return None;
    }
    let mut magnitudes = Vec::with_capacity(half);
    magnitudes.push(0.0);
    for value in buffer.iter().take(half).skip(1) {
        magnitudes.push(value.re.hypot(value.im));
    }
    let mut peak = 1usize;
    for (index, magnitude) in magnitudes.iter().enumerate().skip(2) {
        if *magnitude > magnitudes[peak] {
            peak = index;
        }
    }
    if magnitudes[peak] <= 1e-12 {
        return None;
    }
    let left = magnitudes[peak - 1];
    let centre = magnitudes[peak];
    let right = magnitudes.get(peak + 1).copied().unwrap_or(centre);
    let denominator = left - 2.0 * centre + right;
    let offset = if denominator.abs() > 1e-12 {
        (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    Some((peak as f64 + offset) * f64::from(wav.sample_rate) / size as f64)
}

#[derive(Clone, Copy, Default)]
struct Complex {
    re: f64,
    im: f64,
}

impl Complex {
    fn mul(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }
}

/// In-place iterative Cooley-Tukey FFT; `buffer.len()` must be a power of two.
#[allow(clippy::needless_range_loop)] // The butterflies index mirrored halves by construction.
fn fft(buffer: &mut [Complex]) {
    let size = buffer.len();
    let mut reversed = 0usize;
    for index in 1..size {
        let mut bit = size >> 1;
        while reversed & bit != 0 {
            reversed ^= bit;
            bit >>= 1;
        }
        reversed ^= bit;
        if index < reversed {
            buffer.swap(index, reversed);
        }
    }
    let mut length = 2usize;
    while length <= size {
        let angle = -std::f64::consts::TAU / length as f64;
        let step = Complex {
            re: angle.cos(),
            im: angle.sin(),
        };
        for start in (0..size).step_by(length) {
            let mut twiddle = Complex { re: 1.0, im: 0.0 };
            for offset in 0..length / 2 {
                let even = buffer[start + offset];
                let odd = buffer[start + offset + length / 2].mul(twiddle);
                buffer[start + offset] = Complex {
                    re: even.re + odd.re,
                    im: even.im + odd.im,
                };
                buffer[start + offset + length / 2] = Complex {
                    re: even.re - odd.re,
                    im: even.im - odd.im,
                };
                twiddle = twiddle.mul(step);
            }
        }
        length <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_riff_bytes() {
        assert!(parse_wav(b"not a wave file").is_err());
        assert!(parse_wav(b"").is_err());
    }

    #[test]
    fn rejects_short_fmt() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 4]);
        assert!(parse_wav(&bytes).is_err());
    }

    #[test]
    fn largest_zero_gap_is_in_frames() {
        let wav = Wav {
            sample_rate: 1000,
            channels: 1,
            samples: vec![1, 0, 0, 0, 2],
        };
        let (gap, signal) = longest_zero_gap_ms(&wav);
        assert!(signal);
        assert!((gap - 3.0).abs() < 1e-9);
    }
}
