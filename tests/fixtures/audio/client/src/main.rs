// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot `audio:` null-sink client.
//!
//! Opens two streams, writes 1,000 periods (10 s at 48 kHz) to each, and requires zero underruns.
//! The server's null sink pulls a period only when both streams have one, so the counter is a
//! deterministic proof that the mix kept up.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_audio::{
        Call, Format, SampleFormat, StreamRequest, StreamResponse, StreamState,
    };
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const RATE: u32 = 48_000;
    const CHANNELS: u8 = 2;
    const PERIOD_SAMPLES: usize = 960;
    const PERIODS: usize = 1_000;

    fn call(client: &Client, handle: Handle, call: Call) -> Result<StreamResponse, Errno> {
        let mut request = [0u8; 64];
        let length = StreamRequest::Call(call)
            .encode(&mut request)
            .map_err(|_| Errno::EINVAL)?;
        let mut response = [0u8; 64];
        let length = client.call(handle, &request[..length], &mut response)?;
        StreamResponse::decode(&response[..length]).map_err(|_| Errno::EPROTO)
    }

    fn expect_ok(client: &Client, handle: Handle, call: Call) -> Result<(), Errno> {
        match self::call(client, handle, call)? {
            StreamResponse::Ok => Ok(()),
            StreamResponse::Err(error) => Err(error),
            _ => Err(Errno::EPROTO),
        }
    }

    fn status(client: &Client, handle: Handle) -> Result<(u64, u32, StreamState), Errno> {
        match call(client, handle, Call::GetStatus)? {
            StreamResponse::Status(status) => {
                Ok((status.position_frames, status.underruns, status.state))
            }
            StreamResponse::Err(error) => Err(error),
            _ => Err(Errno::EPROTO),
        }
    }

    fn open(client: &Client) -> Result<Handle, Errno> {
        let mut last = Errno::ENOENT;
        for _ in 0..100 {
            match client.open("audio:out0", 3) {
                Ok(handle) => return Ok(handle),
                Err(error) => {
                    last = error;
                    let _ = raven_rt::nanosleep(50_000_000);
                }
            }
        }
        Err(last)
    }

    fn write_all(client: &Client, handle: Handle, bytes: &[u8]) -> Result<(), Errno> {
        let mut offset = 0usize;
        while offset < bytes.len() {
            match client.write(handle, 0, &bytes[offset..]) {
                Ok(0) => {
                    let _ = raven_rt::nanosleep(1_000_000);
                }
                Ok(count) => offset = offset.saturating_add(count),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn pattern_a(samples: &mut [i16]) {
        for (index, sample) in samples.iter_mut().enumerate() {
            *sample = (index.wrapping_mul(97) % 2000) as i16 - 1000;
        }
    }

    fn pattern_b(samples: &mut [i16]) {
        for (index, sample) in samples.iter_mut().enumerate() {
            *sample = 1500 - (index.wrapping_mul(53) % 1500) as i16;
        }
    }

    fn pcm(samples: &[i16]) -> alloc::vec::Vec<u8> {
        let mut bytes = alloc::vec::Vec::with_capacity(samples.len().saturating_mul(2));
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let client = Client::new();
        let first = match open(&client) {
            Ok(handle) => handle,
            Err(error) => {
                raven_rt::eprintln!("audio-client: no sink: {}", error.description());
                return 1;
            }
        };
        let second = match open(&client) {
            Ok(handle) => handle,
            Err(error) => {
                let _ = client.close(first);
                raven_rt::eprintln!("audio-client: no second stream: {}", error.description());
                return 1;
            }
        };
        let format = Format {
            rate: RATE,
            channels: CHANNELS,
            sample: SampleFormat::S16Le,
        };
        for handle in [first, second] {
            if let Err(error) = expect_ok(&client, handle, Call::SetFormat { format }) {
                raven_rt::eprintln!("audio-client: format: {}", error.description());
                return 1;
            }
            if let Err(error) = expect_ok(
                &client,
                handle,
                Call::SetVolume {
                    left: 32_768,
                    right: 32_768,
                },
            ) {
                raven_rt::eprintln!("audio-client: volume: {}", error.description());
                return 1;
            }
            if let Err(error) = expect_ok(&client, handle, Call::Start) {
                raven_rt::eprintln!("audio-client: start: {}", error.description());
                return 1;
            }
        }
        let mut samples_a = [0i16; PERIOD_SAMPLES];
        let mut samples_b = [0i16; PERIOD_SAMPLES];
        pattern_a(&mut samples_a);
        pattern_b(&mut samples_b);
        let bytes_a = pcm(&samples_a);
        let bytes_b = pcm(&samples_b);
        for period in 0..PERIODS {
            if let Err(error) = write_all(&client, first, &bytes_a) {
                raven_rt::eprintln!("audio-client: write a: {}", error.description());
                return 1;
            }
            if let Err(error) = write_all(&client, second, &bytes_b) {
                raven_rt::eprintln!("audio-client: write b: {}", error.description());
                return 1;
            }
            if period % 100 == 0 {
                for handle in [first, second] {
                    match status(&client, handle) {
                        Ok((_, 0, StreamState::Running)) => {}
                        Ok((position, underruns, state)) => {
                            raven_rt::eprintln!(
                                "audio-client: underrun at period {period}: position={position} underruns={underruns} state={}",
                                state.code()
                            );
                            return 1;
                        }
                        Err(error) => {
                            raven_rt::eprintln!("audio-client: status: {}", error.description());
                            return 1;
                        }
                    }
                }
            }
        }
        let (position, underruns, state) = match status(&client, first) {
            Ok(status) => status,
            Err(error) => {
                raven_rt::eprintln!("audio-client: final status: {}", error.description());
                return 1;
            }
        };
        if position != 480_000 || underruns != 0 || state != StreamState::Running {
            raven_rt::eprintln!(
                "audio-client: bad final status position={position} underruns={underruns} state={}",
                state.code()
            );
            return 1;
        }
        let _ = client.close(first);
        let _ = client.close(second);
        raven_rt::println!("audio-client: ok position={position} underruns={underruns}");
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
