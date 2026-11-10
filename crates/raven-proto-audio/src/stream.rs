// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stream requests and responses for the `audio:` scheme server.
//!
//! `OPEN audio:out<N>` creates a stream. `CALL` carries the typed operations and plain `WRITE`
//! is the simple path that copies PCM bytes into the stream ring. A zero-copy client uses
//! `SET_BUFFER` instead, transferring a shared memory object and then referring to it by the
//! destination-valid handle (see the module documentation in `README.md`).
//!
//! Every message starts with a little-endian `u16` version. Lengths and counts are bounded before
//! use; decoders return [`crate::DecodeError`] rather than panicking.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::DecodeError;
use crate::format::{Format, SampleFormat};
use crate::ring::{HEADER_LEN, MAX_FRAMES, MAX_PERIODS, MAX_REGION};
use crate::wire::{Reader, WireError, Writer};

/// Version of the audio stream message framing.
pub const PROTOCOL_VERSION: u16 = 1;

/// Request kind: `OPEN audio:out<N>`.
pub const KIND_OPEN: u16 = 1;
/// Request kind: plain PCM `WRITE`.
pub const KIND_WRITE: u16 = 2;
/// Request kind: typed `CALL`.
pub const KIND_CALL: u16 = 3;

/// `CALL` operation: `SET_FORMAT`.
pub const OP_SET_FORMAT: u16 = 1;
/// `CALL` operation: `SET_BUFFER`.
pub const OP_SET_BUFFER: u16 = 2;
/// `CALL` operation: `START`.
pub const OP_START: u16 = 3;
/// `CALL` operation: `STOP`.
pub const OP_STOP: u16 = 4;
/// `CALL` operation: `DRAIN`.
pub const OP_DRAIN: u16 = 5;
/// `CALL` operation: `SET_VOLUME`.
pub const OP_SET_VOLUME: u16 = 6;
/// `CALL` operation: `GET_STATUS`.
pub const OP_GET_STATUS: u16 = 7;

/// Maximum length of an `OPEN` path in bytes.
pub const MAX_PATH: usize = 32;
/// Maximum `WRITE` payload in bytes.
pub const MAX_WRITE: usize = 65_536;

/// Response tag: success without a body.
const TAG_OK: u16 = 0;
/// Response tag: stream id.
const TAG_STREAM: u16 = 1;
/// Response tag: granted buffer geometry.
const TAG_BUFFER: u16 = 2;
/// Response tag: playback status.
const TAG_STATUS: u16 = 3;
/// Response tag: errno.
const TAG_ERR: u16 = 4;

/// Playback state reported by `GET_STATUS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum StreamState {
    /// Stopped; the ring is not being consumed.
    Stopped = 0,
    /// Running; the driver consumes periods as they become available.
    Running = 1,
    /// Draining; no new writes are accepted, the ring is being played out.
    Draining = 2,
    /// The driver found too few frames for a period.
    Underrun = 3,
}

impl StreamState {
    /// Wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decode a wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Stopped),
            1 => Some(Self::Running),
            2 => Some(Self::Draining),
            3 => Some(Self::Underrun),
            _ => None,
        }
    }
}

/// Granted shared-ring geometry returned by `SET_BUFFER`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferInfo {
    /// Frames per period.
    pub period_frames: u32,
    /// Number of periods in the ring.
    pub period_count: u16,
    /// Minimum region size of the attached memory object in bytes.
    pub region_len: u32,
}

/// Playback status returned by `GET_STATUS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    /// Frames played since the last `START`.
    pub position_frames: u64,
    /// Periods the driver could not fill since the stream was configured.
    pub underruns: u32,
    /// Current playback state.
    pub state: StreamState,
}

/// A typed `CALL` operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// Select the stream format. Reserved formats are accepted on the wire so the server can
    /// answer `ENOTSUP`.
    SetFormat {
        /// Requested format.
        format: Format,
    },
    /// Attach a shared PCM ring.
    ///
    /// `handle` is the destination-valid handle produced by `HANDLE_TRANSFER` for the memory
    /// object; it is never a pointer. `period_frames * period_count` is the ring capacity in
    /// frames.
    SetBuffer {
        /// Destination-valid handle of the attached memory object.
        handle: Handle,
        /// Frames per period.
        period_frames: u32,
        /// Number of periods in the ring.
        period_count: u16,
    },
    /// Start or resume playback.
    Start,
    /// Stop playback, keeping buffered data.
    Stop,
    /// Play out the remaining buffered data, then stop.
    Drain,
    /// Set the left and right volume, `0..=65535`.
    SetVolume {
        /// Left channel volume.
        left: u16,
        /// Right channel volume.
        right: u16,
    },
    /// Return position, underruns and state.
    GetStatus,
}

impl Call {
    /// Check the operation against the implemented feature set.
    ///
    /// This is the server-side gate: `EINVAL` for an out-of-range rate or channel count and
    /// `ENOTSUP` for a reserved sample format. Framing already rejects values outside the wire
    /// bounds, so this only adds the "implemented yet" decision.
    pub fn check(&self) -> Result<(), Errno> {
        match self {
            Self::SetFormat { format } => format.check(),
            Self::SetBuffer {
                handle,
                period_frames,
                period_count,
            } => {
                if *handle == Handle::NONE
                    || *period_frames == 0
                    || *period_frames > MAX_FRAMES
                    || *period_count == 0
                    || *period_count > MAX_PERIODS
                    || u64::from(*period_frames) * u64::from(*period_count) > u64::from(MAX_FRAMES)
                {
                    return Err(Errno::EINVAL);
                }
                Ok(())
            }
            Self::Start | Self::Stop | Self::Drain | Self::GetStatus => Ok(()),
            Self::SetVolume { .. } => Ok(()),
        }
    }
}

/// A request sent to `audiod`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamRequest<'a> {
    /// Open a stream on an output device.
    Open {
        /// Path `audio:out<N>`, `1..=MAX_PATH` bytes.
        path: &'a str,
    },
    /// Write PCM bytes; `audiod` copies them into the stream ring.
    Write {
        /// PCM payload, at most `MAX_WRITE` bytes.
        data: &'a [u8],
    },
    /// Run a typed operation.
    Call(Call),
}

impl<'a> StreamRequest<'a> {
    /// Encode the request into `out` and return the byte count.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut writer = Writer::new(out);
        writer.put_u16(PROTOCOL_VERSION)?;
        match self {
            Self::Open { path } => {
                if path.is_empty() || path.len() > MAX_PATH {
                    return Err(WireError::TooLong);
                }
                if crate::path::parse(path).is_err() {
                    return Err(WireError::Invalid);
                }
                writer.put_u16(KIND_OPEN)?;
                writer.put_u16(path.len() as u16)?;
                writer.put_bytes(path.as_bytes())?;
            }
            Self::Write { data } => {
                if data.len() > MAX_WRITE {
                    return Err(WireError::TooLong);
                }
                writer.put_u16(KIND_WRITE)?;
                writer.put_u32(data.len() as u32)?;
                writer.put_bytes(data)?;
            }
            Self::Call(call) => {
                writer.put_u16(KIND_CALL)?;
                call.write(&mut writer)?;
            }
        }
        Ok(writer.position())
    }

    /// Decode one request. The entire slice must belong to the request.
    pub fn decode(buffer: &'a [u8]) -> Result<StreamRequest<'a>, DecodeError> {
        let mut reader = Reader::new(buffer);
        let version = reader.get_u16().map_err(DecodeError::from)?;
        if version != PROTOCOL_VERSION {
            return Err(DecodeError::version());
        }
        let kind = reader.get_u16().map_err(DecodeError::from)?;
        let request = match kind {
            KIND_OPEN => {
                let length = reader.get_u16().map_err(DecodeError::from)? as usize;
                if length == 0 || length > MAX_PATH {
                    return Err(DecodeError::length());
                }
                let bytes = reader.get_bytes(length).map_err(DecodeError::from)?;
                let path = core::str::from_utf8(bytes).map_err(|_| DecodeError::malformed())?;
                if crate::path::parse(path).is_err() {
                    return Err(DecodeError::malformed());
                }
                Self::Open { path }
            }
            KIND_WRITE => {
                let length = reader.get_u32().map_err(DecodeError::from)? as usize;
                if length > MAX_WRITE {
                    return Err(DecodeError::length());
                }
                Self::Write {
                    data: reader.get_bytes(length).map_err(DecodeError::from)?,
                }
            }
            KIND_CALL => Self::Call(Call::read(&mut reader)?),
            _ => return Err(DecodeError::tag()),
        };
        if !reader.is_empty() {
            return Err(DecodeError::trailing());
        }
        Ok(request)
    }
}

impl Call {
    fn write(&self, writer: &mut Writer<'_>) -> Result<(), WireError> {
        match self {
            Self::SetFormat { format } => {
                if format.check_bounds().is_err() {
                    return Err(WireError::Invalid);
                }
                writer.put_u16(OP_SET_FORMAT)?;
                writer.put_u32(format.rate)?;
                writer.put_u8(format.channels)?;
                writer.put_u8(format.sample.code())
            }
            Self::SetBuffer {
                handle,
                period_frames,
                period_count,
            } => {
                if self.check().is_err() {
                    return Err(WireError::Invalid);
                }
                writer.put_u16(OP_SET_BUFFER)?;
                writer.put_u32(handle.0)?;
                writer.put_u32(*period_frames)?;
                writer.put_u16(*period_count)
            }
            Self::Start => writer.put_u16(OP_START),
            Self::Stop => writer.put_u16(OP_STOP),
            Self::Drain => writer.put_u16(OP_DRAIN),
            Self::SetVolume { left, right } => {
                writer.put_u16(OP_SET_VOLUME)?;
                writer.put_u16(*left)?;
                writer.put_u16(*right)
            }
            Self::GetStatus => writer.put_u16(OP_GET_STATUS),
        }
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let op = reader.get_u16().map_err(DecodeError::from)?;
        let call = match op {
            OP_SET_FORMAT => {
                let rate = reader.get_u32().map_err(DecodeError::from)?;
                let channels = reader.get_u8().map_err(DecodeError::from)?;
                let sample = SampleFormat::from_code(reader.get_u8().map_err(DecodeError::from)?)
                    .ok_or_else(DecodeError::tag)?;
                let format = Format {
                    rate,
                    channels,
                    sample,
                };
                format.check_bounds().map_err(|_| DecodeError::length())?;
                Self::SetFormat { format }
            }
            OP_SET_BUFFER => {
                let handle = Handle(reader.get_u32().map_err(DecodeError::from)?);
                let period_frames = reader.get_u32().map_err(DecodeError::from)?;
                let period_count = reader.get_u16().map_err(DecodeError::from)?;
                if handle == Handle::NONE {
                    return Err(DecodeError::malformed());
                }
                if period_frames == 0
                    || period_frames > MAX_FRAMES
                    || period_count == 0
                    || period_count > MAX_PERIODS
                    || u64::from(period_frames) * u64::from(period_count) > u64::from(MAX_FRAMES)
                {
                    return Err(DecodeError::length());
                }
                Self::SetBuffer {
                    handle,
                    period_frames,
                    period_count,
                }
            }
            OP_START => Self::Start,
            OP_STOP => Self::Stop,
            OP_DRAIN => Self::Drain,
            OP_SET_VOLUME => Self::SetVolume {
                left: reader.get_u16().map_err(DecodeError::from)?,
                right: reader.get_u16().map_err(DecodeError::from)?,
            },
            OP_GET_STATUS => Self::GetStatus,
            _ => return Err(DecodeError::tag()),
        };
        Ok(call)
    }
}

/// A response from `audiod`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamResponse {
    /// Success without a body.
    Ok,
    /// A stream id; the attached handle travelled through the kernel.
    Stream {
        /// Numeric stream id.
        id: u32,
    },
    /// Granted buffer geometry.
    Buffer(BufferInfo),
    /// Playback status.
    Status(Status),
    /// Failure carrying the errno the caller must report.
    Err(Errno),
}

impl StreamResponse {
    /// Build an error response.
    pub const fn error(errno: Errno) -> Self {
        Self::Err(errno)
    }

    /// Whether this response is an error.
    pub const fn is_error(self) -> bool {
        matches!(self, Self::Err(_))
    }

    /// Encode the response into `out` and return the byte count.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut writer = Writer::new(out);
        writer.put_u16(PROTOCOL_VERSION)?;
        match self {
            Self::Ok => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_OK)?;
            }
            Self::Stream { id } => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_STREAM)?;
                writer.put_u32(*id)?;
            }
            Self::Buffer(info) => {
                if info.period_frames == 0
                    || info.period_frames > MAX_FRAMES
                    || info.period_count == 0
                    || info.period_count > MAX_PERIODS
                    || u64::from(info.period_frames) * u64::from(info.period_count)
                        > u64::from(MAX_FRAMES)
                    || info.region_len < HEADER_LEN as u32
                    || info.region_len > MAX_REGION as u32
                {
                    return Err(WireError::Invalid);
                }
                writer.put_u16(0)?;
                writer.put_u16(TAG_BUFFER)?;
                writer.put_u32(info.period_frames)?;
                writer.put_u16(info.period_count)?;
                writer.put_u32(info.region_len)?;
            }
            Self::Status(status) => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_STATUS)?;
                writer.put_u64(status.position_frames)?;
                writer.put_u32(status.underruns)?;
                writer.put_u8(status.state.code())?;
            }
            Self::Err(errno) => {
                writer.put_u16(errno.0)?;
                writer.put_u16(TAG_ERR)?;
            }
        }
        Ok(writer.position())
    }

    /// Decode one response. The entire slice must belong to the response.
    pub fn decode(buffer: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::new(buffer);
        let version = reader.get_u16().map_err(DecodeError::from)?;
        if version != PROTOCOL_VERSION {
            return Err(DecodeError::version());
        }
        let status = reader.get_u16().map_err(DecodeError::from)?;
        let tag = reader.get_u16().map_err(DecodeError::from)?;
        let response = match tag {
            TAG_OK => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Ok
            }
            TAG_STREAM => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Stream {
                    id: reader.get_u32().map_err(DecodeError::from)?,
                }
            }
            TAG_BUFFER => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                let period_frames = reader.get_u32().map_err(DecodeError::from)?;
                let period_count = reader.get_u16().map_err(DecodeError::from)?;
                let region_len = reader.get_u32().map_err(DecodeError::from)?;
                if period_frames == 0
                    || period_frames > MAX_FRAMES
                    || period_count == 0
                    || period_count > MAX_PERIODS
                    || u64::from(period_frames) * u64::from(period_count) > u64::from(MAX_FRAMES)
                    || region_len < HEADER_LEN as u32
                    || region_len > MAX_REGION as u32
                {
                    return Err(DecodeError::length());
                }
                Self::Buffer(BufferInfo {
                    period_frames,
                    period_count,
                    region_len,
                })
            }
            TAG_STATUS => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                let position_frames = reader.get_u64().map_err(DecodeError::from)?;
                let underruns = reader.get_u32().map_err(DecodeError::from)?;
                let state = StreamState::from_code(reader.get_u8().map_err(DecodeError::from)?)
                    .ok_or_else(DecodeError::tag)?;
                Self::Status(Status {
                    position_frames,
                    underruns,
                    state,
                })
            }
            TAG_ERR => {
                if status == 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Err(Errno(status))
            }
            _ => return Err(DecodeError::tag()),
        };
        if !reader.is_empty() {
            return Err(DecodeError::trailing());
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_request(request: StreamRequest<'_>) -> Vec<u8> {
        let mut buffer = [0u8; 256];
        let length = request.encode(&mut buffer).unwrap();
        buffer[..length].to_vec()
    }

    fn encode_response(response: StreamResponse) -> Vec<u8> {
        let mut buffer = [0u8; 256];
        let length = response.encode(&mut buffer).unwrap();
        buffer[..length].to_vec()
    }

    const STEREO_48K: Format = Format {
        rate: 48_000,
        channels: 2,
        sample: SampleFormat::S16Le,
    };

    #[test]
    fn open_golden_bytes() {
        assert_eq!(
            encode_request(StreamRequest::Open { path: "audio:out0" }),
            [
                0x01, 0x00, 0x01, 0x00, 0x0a, 0x00, b'a', b'u', b'd', b'i', b'o', b':', b'o', b'u',
                b't', b'0'
            ]
        );
    }

    #[test]
    fn write_golden_bytes() {
        assert_eq!(
            encode_request(StreamRequest::Write {
                data: &[1, 2, 3, 4]
            }),
            [0x01, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 1, 2, 3, 4]
        );
    }

    #[test]
    fn set_format_golden_bytes() {
        assert_eq!(
            encode_request(StreamRequest::Call(Call::SetFormat { format: STEREO_48K })),
            [
                0x01, 0x00, 0x03, 0x00, 0x01, 0x00, 0x80, 0xbb, 0x00, 0x00, 0x02, 0x01
            ]
        );
    }

    #[test]
    fn set_buffer_golden_bytes() {
        assert_eq!(
            encode_request(StreamRequest::Call(Call::SetBuffer {
                handle: Handle(7),
                period_frames: 1024,
                period_count: 4,
            })),
            [
                0x01, 0x00, 0x03, 0x00, 0x02, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00,
                0x04, 0x00
            ]
        );
    }

    #[test]
    fn set_volume_and_get_status_golden_bytes() {
        assert_eq!(
            encode_request(StreamRequest::Call(Call::SetVolume {
                left: 32_768,
                right: 65_535
            })),
            [0x01, 0x00, 0x03, 0x00, 0x06, 0x00, 0x00, 0x80, 0xff, 0xff]
        );
        assert_eq!(
            encode_request(StreamRequest::Call(Call::GetStatus)),
            [0x01, 0x00, 0x03, 0x00, 0x07, 0x00]
        );
    }

    #[test]
    fn start_stop_drain_golden_bytes() {
        assert_eq!(
            encode_request(StreamRequest::Call(Call::Start)),
            [0x01, 0x00, 0x03, 0x00, 0x03, 0x00]
        );
        assert_eq!(
            encode_request(StreamRequest::Call(Call::Stop)),
            [0x01, 0x00, 0x03, 0x00, 0x04, 0x00]
        );
        assert_eq!(
            encode_request(StreamRequest::Call(Call::Drain)),
            [0x01, 0x00, 0x03, 0x00, 0x05, 0x00]
        );
    }

    #[test]
    fn request_roundtrips() {
        for request in [
            StreamRequest::Open { path: "audio:out3" },
            StreamRequest::Write { data: &[0; 16] },
            StreamRequest::Call(Call::SetFormat { format: Format::CD }),
            StreamRequest::Call(Call::SetBuffer {
                handle: Handle(9),
                period_frames: 512,
                period_count: 8,
            }),
            StreamRequest::Call(Call::Start),
            StreamRequest::Call(Call::Stop),
            StreamRequest::Call(Call::Drain),
            StreamRequest::Call(Call::SetVolume {
                left: 0,
                right: 65_535,
            }),
            StreamRequest::Call(Call::GetStatus),
        ] {
            let bytes = encode_request(request);
            assert_eq!(StreamRequest::decode(&bytes), Ok(request), "{request:?}");
        }
    }

    #[test]
    fn status_response_golden_bytes() {
        assert_eq!(
            encode_response(StreamResponse::Status(Status {
                position_frames: 1_000,
                underruns: 2,
                state: StreamState::Running,
            })),
            [
                0x01, 0x00, 0x00, 0x00, 0x03, 0x00, 0xe8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x02, 0x00, 0x00, 0x00, 0x01
            ]
        );
    }

    #[test]
    fn buffer_response_golden_bytes() {
        assert_eq!(
            encode_response(StreamResponse::Buffer(BufferInfo {
                period_frames: 1024,
                period_count: 4,
                region_len: 16_512,
            })),
            [
                0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x04, 0x00, 0x00, 0x04, 0x00, 0x80, 0x40,
                0x00, 0x00
            ]
        );
    }

    #[test]
    fn error_response_golden_bytes() {
        assert_eq!(
            encode_response(StreamResponse::error(crate::ENOTSUP)),
            [0x01, 0x00, 0x5f, 0x00, 0x04, 0x00]
        );
    }

    #[test]
    fn responses_roundtrip() {
        for response in [
            StreamResponse::Ok,
            StreamResponse::Stream { id: 3 },
            StreamResponse::Buffer(BufferInfo {
                period_frames: 256,
                period_count: 2,
                region_len: 2_176,
            }),
            StreamResponse::Status(Status {
                position_frames: u64::MAX,
                underruns: 9,
                state: StreamState::Underrun,
            }),
            StreamResponse::Err(Errno::EINVAL),
        ] {
            let bytes = encode_response(response);
            assert_eq!(StreamResponse::decode(&bytes), Ok(response), "{response:?}");
        }
    }

    #[test]
    fn reserved_formats_are_accepted_on_the_wire_and_rejected_by_check() {
        for sample in [
            SampleFormat::S24Le,
            SampleFormat::S32Le,
            SampleFormat::F32Le,
        ] {
            let format = Format {
                rate: 48_000,
                channels: 2,
                sample,
            };
            let bytes = encode_request(StreamRequest::Call(Call::SetFormat { format }));
            assert_eq!(
                StreamRequest::decode(&bytes),
                Ok(StreamRequest::Call(Call::SetFormat { format }))
            );
            assert_eq!(Call::SetFormat { format }.check(), Err(crate::ENOTSUP));
        }
    }

    #[test]
    fn out_of_range_formats_are_rejected_while_decoding() {
        fn frame(rate: u32, channels: u8, sample: u8) -> Vec<u8> {
            let mut bytes = vec![0x01, 0x00, 0x03, 0x00, 0x01, 0x00];
            bytes.extend_from_slice(&rate.to_le_bytes());
            bytes.push(channels);
            bytes.push(sample);
            bytes
        }
        for rate in [0, 7_999, 192_001, u32::MAX] {
            assert_eq!(
                StreamRequest::decode(&frame(rate, 2, 1)),
                Err(DecodeError::length()),
                "rate {rate}"
            );
        }
        for channels in [0, 9, 255] {
            assert_eq!(
                StreamRequest::decode(&frame(48_000, channels, 1)),
                Err(DecodeError::length()),
                "channels {channels}"
            );
        }
    }

    #[test]
    fn unknown_sample_format_is_rejected() {
        let bytes = [
            0x01, 0x00, 0x03, 0x00, 0x01, 0x00, 0x80, 0xbb, 0x00, 0x00, 0x02, 0x09,
        ];
        assert_eq!(StreamRequest::decode(&bytes), Err(DecodeError::tag()));
    }

    #[test]
    fn set_buffer_rejects_none_and_oversized_geometry() {
        let mut buffer = [0u8; 64];
        for call in [
            Call::SetBuffer {
                handle: Handle::NONE,
                period_frames: 1,
                period_count: 1,
            },
            Call::SetBuffer {
                handle: Handle(1),
                period_frames: 0,
                period_count: 1,
            },
            Call::SetBuffer {
                handle: Handle(1),
                period_frames: MAX_FRAMES,
                period_count: 2,
            },
        ] {
            assert_eq!(call.check(), Err(Errno::EINVAL));
            assert_eq!(
                StreamRequest::Call(call).encode(&mut buffer),
                Err(WireError::Invalid)
            );
        }
    }

    #[test]
    fn unknown_version_kind_and_tag_are_rejected() {
        let mut bytes = encode_request(StreamRequest::Call(Call::Start));
        bytes[0] = 2;
        assert_eq!(StreamRequest::decode(&bytes), Err(DecodeError::version()));
        let mut bytes = encode_request(StreamRequest::Call(Call::Start));
        bytes[2] = 9;
        assert_eq!(StreamRequest::decode(&bytes), Err(DecodeError::tag()));
        let bytes = [0x01, 0x00, 0x00, 0x00, 0x09, 0x00];
        assert_eq!(StreamResponse::decode(&bytes), Err(DecodeError::tag()));
    }

    #[test]
    fn truncation_and_trailing_bytes_are_rejected() {
        let bytes = encode_request(StreamRequest::Call(Call::SetVolume { left: 1, right: 2 }));
        for length in 0..bytes.len() {
            assert!(StreamRequest::decode(&bytes[..length]).is_err(), "{length}");
        }
        let mut bytes = encode_response(StreamResponse::Ok);
        bytes.push(0);
        assert_eq!(StreamResponse::decode(&bytes), Err(DecodeError::trailing()));
        let mut bytes = encode_request(StreamRequest::Call(Call::Start));
        bytes.push(0);
        assert_eq!(StreamRequest::decode(&bytes), Err(DecodeError::trailing()));
    }

    #[test]
    fn open_rejects_bad_paths() {
        let mut buffer = [0u8; 64];
        assert_eq!(
            StreamRequest::Open { path: "" }.encode(&mut buffer),
            Err(WireError::TooLong)
        );
        assert_eq!(
            StreamRequest::Open { path: "net:lo" }.encode(&mut buffer),
            Err(WireError::Invalid)
        );
    }

    #[test]
    fn write_is_bounded() {
        let data = vec![0u8; MAX_WRITE + 1];
        let mut buffer = vec![0u8; MAX_WRITE + 16];
        assert_eq!(
            StreamRequest::Write { data: &data }.encode(&mut buffer),
            Err(WireError::TooLong)
        );
        let bytes = [
            0x01, 0x00, 0x02, 0x00, 0x01, 0x00, 0x01, 0x00, // 65537 bytes declared
        ];
        assert_eq!(StreamRequest::decode(&bytes), Err(DecodeError::length()));
    }
}
