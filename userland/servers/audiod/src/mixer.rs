// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stream state and the integer mixer behind `audio:out0`.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_audio_core::format::{downmix_mono, mono_to_stereo};
use raven_audio_core::mix::{self, GAIN_UNITY};
use raven_audio_core::resample::resample_linear;
use raven_proto_audio::stream::{Call, Status, StreamState};
use raven_proto_audio::{Format, SampleFormat};

/// The device sample rate.
pub const DEVICE_RATE: u32 = 48_000;
/// The device channel count.
pub const DEVICE_CHANNELS: u8 = 2;
/// Frames per null-sink period (10 ms at 48 kHz).
pub const PERIOD_FRAMES: u32 = 480;
/// The per-stream buffer is bounded to two seconds at the stream rate.
pub const MAX_BUFFER_SECONDS: u32 = 2;

/// One stream as seen by `GET_STATUS` and the tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamView {
    /// Numeric stream id.
    pub id: u64,
    /// Stream format (rate, channels, S16LE).
    pub format: Format,
    /// Left volume in Q15.
    pub volume_left: u16,
    /// Right volume in Q15.
    pub volume_right: u16,
    /// State.
    pub state: StreamState,
    /// Frames consumed since the last START.
    pub position_frames: u64,
    /// Short periods since the stream was opened.
    pub underruns: u32,
    /// Buffered frames at the stream rate.
    pub buffered_frames: usize,
}

/// One client stream.
#[derive(Clone, Debug)]
pub struct Stream {
    /// Numeric id (assigned by [`Mixer::open`]).
    pub id: u64,
    /// Format selected by `SET_FORMAT`.
    pub format: Format,
    /// Left volume in Q15.
    pub volume_left: u16,
    /// Right volume in Q15.
    pub volume_right: u16,
    /// Playback state.
    pub state: StreamState,
    /// Frames consumed since the last START.
    pub position_frames: u64,
    /// Short periods.
    pub underruns: u32,
    samples: VecDeque<i16>,
}

impl Stream {
    fn new(id: u64) -> Self {
        Self {
            id,
            format: Format {
                rate: DEVICE_RATE,
                channels: DEVICE_CHANNELS,
                sample: SampleFormat::S16Le,
            },
            volume_left: GAIN_UNITY,
            volume_right: GAIN_UNITY,
            state: StreamState::Stopped,
            position_frames: 0,
            underruns: 0,
            samples: VecDeque::new(),
        }
    }

    /// A snapshot for status and tests.
    #[must_use]
    pub fn view(&self) -> StreamView {
        StreamView {
            id: self.id,
            format: self.format,
            volume_left: self.volume_left,
            volume_right: self.volume_right,
            state: self.state,
            position_frames: self.position_frames,
            underruns: self.underruns,
            buffered_frames: self.buffered_frames(),
        }
    }

    /// Buffered frames at the stream rate.
    #[must_use]
    pub fn buffered_frames(&self) -> usize {
        let channels = usize::from(self.format.channels.max(1));
        self.samples.len() / channels
    }

    fn max_samples(&self) -> usize {
        let frames = usize::try_from(self.format.rate)
            .unwrap_or(usize::MAX / 2)
            .saturating_mul(MAX_BUFFER_SECONDS as usize);
        frames.saturating_mul(usize::from(self.format.channels.max(1)))
    }

    fn in_frames_for(&self, out_frames: usize) -> usize {
        if out_frames == 0 {
            return 0;
        }
        let scaled = (out_frames as u128).saturating_mul(u128::from(self.format.rate));
        let frames = scaled.saturating_add(u128::from(DEVICE_RATE).saturating_sub(1))
            / u128::from(DEVICE_RATE);
        usize::try_from(frames).unwrap_or(usize::MAX).max(1)
    }

    /// Append PCM bytes; returns the number of bytes accepted (even).
    pub fn write(&mut self, bytes: &[u8]) -> usize {
        let channels = usize::from(self.format.channels.max(1));
        let max_samples = self.max_samples();
        let free = max_samples.saturating_sub(self.samples.len());
        let free_frames = free / channels;
        let requested_frames = bytes.len() / (channels * 2);
        let frames = requested_frames.min(free_frames);
        for chunk in bytes.chunks_exact(2).take(frames.saturating_mul(channels)) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            self.samples.push_back(sample);
        }
        frames.saturating_mul(channels).saturating_mul(2)
    }

    /// Pull `out.len()/2` stereo frames, applying volume and channel conversion.
    ///
    /// A short stream contributes silence after its buffered frames. `draining` marks the final
    /// partial period of `DRAIN`: it is not an underrun.
    fn pull_stereo(&mut self, out: &mut [i16], draining: bool) -> bool {
        out.fill(0);
        let out_frames = out.len() / 2;
        if out_frames == 0 {
            return false;
        }
        let channels = usize::from(self.format.channels.max(1));
        let in_frames = self.in_frames_for(out_frames);
        let available = self.buffered_frames();
        let short = available < in_frames;
        let use_frames = available.min(in_frames);
        if short && !draining {
            self.underruns = self.underruns.saturating_add(1);
            self.state = StreamState::Underrun;
        }
        let mut input: Vec<i16> = Vec::with_capacity(in_frames.saturating_mul(channels));
        for _ in 0..use_frames.saturating_mul(channels) {
            input.push(self.samples.pop_front().unwrap_or(0));
        }
        input.resize(in_frames.saturating_mul(channels), 0);
        let resampled = if self.format.rate == DEVICE_RATE {
            input
        } else {
            resample_linear(
                &input,
                self.format.channels.max(1),
                self.format.rate,
                DEVICE_RATE,
            )
            .unwrap_or_else(|| vec![0i16; out.len()])
        };
        let stereo_frames = resampled.len() / channels;
        match channels {
            2 => {
                let count = stereo_frames.min(out_frames).saturating_mul(2);
                if let Some(target) = out.get_mut(..count)
                    && let Some(source) = resampled.get(..count)
                {
                    target.copy_from_slice(source);
                }
            }
            1 => {
                let count = mono_to_stereo(resampled.get(..stereo_frames).unwrap_or(&[]), out);
                let _ = count;
            }
            _ => {
                let mut mono = vec![0i16; stereo_frames];
                let count = downmix_mono(
                    resampled
                        .get(..stereo_frames.saturating_mul(channels))
                        .unwrap_or(&[]),
                    self.format.channels.max(1),
                    &mut mono,
                );
                let _ = mono_to_stereo(mono.get(..count).unwrap_or(&[]), out);
            }
        }
        for frame in out.chunks_exact_mut(2) {
            let left =
                (i64::from(frame[0]) * i64::from(self.volume_left.clamp(0, GAIN_UNITY))) >> 15;
            let right =
                (i64::from(frame[1]) * i64::from(self.volume_right.clamp(0, GAIN_UNITY))) >> 15;
            frame[0] = clamp_i16(left);
            frame[1] = clamp_i16(right);
        }
        if !short && self.state == StreamState::Underrun {
            self.state = StreamState::Running;
        }
        if draining && self.samples.is_empty() {
            self.state = StreamState::Stopped;
        }
        short
    }
}

fn clamp_i16(value: i64) -> i16 {
    if value > i64::from(i16::MAX) {
        i16::MAX
    } else if value < i64::from(i16::MIN) {
        i16::MIN
    } else {
        value as i16
    }
}

/// The `audio:out0` device state.
#[derive(Clone, Debug)]
pub struct Mixer {
    streams: BTreeMap<u64, Stream>,
    next_id: u64,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    /// A mixer with no streams.
    #[must_use]
    pub fn new() -> Self {
        Self {
            streams: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Open a stream on the null sink.
    pub fn open(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.streams.insert(id, Stream::new(id));
        id
    }

    /// Close a stream.
    pub fn close(&mut self, id: u64) -> Result<(), Errno> {
        if self.streams.remove(&id).is_some() {
            Ok(())
        } else {
            Err(Errno::EBADF)
        }
    }

    /// One stream, or `EBADF`.
    pub fn stream(&self, id: u64) -> Result<&Stream, Errno> {
        self.streams.get(&id).ok_or(Errno::EBADF)
    }

    /// A snapshot of one stream.
    pub fn view(&self, id: u64) -> Result<StreamView, Errno> {
        self.stream(id).map(Stream::view)
    }

    /// Append PCM to a stream. `EIO` once the stream is draining.
    pub fn write(&mut self, id: u64, bytes: &[u8]) -> Result<usize, Errno> {
        let stream = self.streams.get_mut(&id).ok_or(Errno::EBADF)?;
        if stream.state == StreamState::Draining {
            return Err(Errno::EIO);
        }
        Ok(stream.write(bytes))
    }

    /// Apply one typed operation.
    pub fn call(&mut self, id: u64, call: Call) -> Result<Option<Status>, Errno> {
        call.check()?;
        match call {
            Call::SetFormat { format } => {
                if format.sample != SampleFormat::S16Le {
                    return Err(Errno::EOPNOTSUPP);
                }
                let stream = self.streams.get_mut(&id).ok_or(Errno::EBADF)?;
                stream.format = format;
                stream.samples.clear();
                Ok(None)
            }
            Call::SetBuffer { .. } => Err(Errno::EOPNOTSUPP),
            Call::Start => {
                let stream = self.streams.get_mut(&id).ok_or(Errno::EBADF)?;
                stream.state = StreamState::Running;
                stream.position_frames = 0;
                self.pump();
                Ok(None)
            }
            Call::Stop => {
                let stream = self.streams.get_mut(&id).ok_or(Errno::EBADF)?;
                stream.state = StreamState::Stopped;
                Ok(None)
            }
            Call::Drain => {
                let stream = self.streams.get_mut(&id).ok_or(Errno::EBADF)?;
                stream.state = StreamState::Draining;
                self.pump();
                Ok(None)
            }
            Call::SetVolume { left, right } => {
                let stream = self.streams.get_mut(&id).ok_or(Errno::EBADF)?;
                stream.volume_left = left.min(GAIN_UNITY);
                stream.volume_right = right.min(GAIN_UNITY);
                Ok(None)
            }
            Call::GetStatus => {
                let stream = self.streams.get(&id).ok_or(Errno::EBADF)?;
                Ok(Some(Status {
                    position_frames: stream.position_frames,
                    underruns: stream.underruns,
                    state: stream.state,
                }))
            }
        }
    }

    /// Mix one period into `out` (stereo interleaved, `out.len()/2` frames).
    pub fn mix_period(&mut self, out: &mut [i16]) -> Result<(), Errno> {
        out.fill(0);
        let frames = out.len() / 2;
        if frames == 0 {
            return Ok(());
        }
        let ids: Vec<u64> = self
            .streams
            .iter()
            .filter(|(_, stream)| {
                matches!(
                    stream.state,
                    StreamState::Running | StreamState::Underrun | StreamState::Draining
                )
            })
            .map(|(id, _)| *id)
            .collect();
        let mut buffers: Vec<Vec<i16>> = Vec::new();
        for id in &ids {
            let Some(stream) = self.streams.get_mut(id) else {
                continue;
            };
            let draining = stream.state == StreamState::Draining;
            let mut buffer = vec![0i16; out.len()];
            let _ = stream.pull_stereo(&mut buffer, draining);
            stream.position_frames = stream.position_frames.saturating_add(frames as u64);
            buffers.push(buffer);
        }
        let streams: Vec<mix::Stream<'_>> = buffers
            .iter()
            .map(|buffer| mix::Stream::new(buffer, GAIN_UNITY))
            .collect();
        mix::mix(&streams, out);
        for id in &ids {
            if let Some(stream) = self.streams.get_mut(id)
                && stream.state == StreamState::Draining
                && stream.samples.is_empty()
            {
                stream.state = StreamState::Stopped;
            }
        }
        Ok(())
    }

    /// Drain every running stream into the null sink while it has a full period.
    ///
    /// The null sink is pull-on-demand: a period is mixed only when every running stream has
    /// enough frames, so a cooperative client pair never underruns. Returns the periods mixed.
    pub fn pump(&mut self) -> u32 {
        let mut periods = 0u32;
        loop {
            let running: Vec<&Stream> = self
                .streams
                .values()
                .filter(|stream| {
                    matches!(stream.state, StreamState::Running | StreamState::Underrun)
                })
                .collect();
            let draining = self
                .streams
                .values()
                .any(|stream| stream.state == StreamState::Draining);
            if running.is_empty() && !draining {
                break;
            }
            let running_ready = running.iter().all(|stream| {
                stream.buffered_frames() >= stream.in_frames_for(PERIOD_FRAMES as usize)
            });
            if !running_ready {
                break;
            }
            let mut out = vec![0i16; PERIOD_FRAMES as usize * 2];
            let _ = self.mix_period(&mut out);
            periods = periods.saturating_add(1);
            if periods > 1_000_000 {
                break;
            }
            if self
                .streams
                .values()
                .all(|stream| stream.state == StreamState::Stopped)
            {
                break;
            }
        }
        periods
    }
}
