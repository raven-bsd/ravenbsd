// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `beep`: a generated tone on `audio:out0`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::String;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_audio_core::tone::{Tone, Waveform};
    use raven_proto_audio::{Call, Format, SampleFormat, StreamRequest, StreamResponse};
    use raven_rt::entry;
    use raven_scheme::Client;

    use raven_beep::{Options, parse_args, summary};

    entry!(main);

    const RATE: u32 = 48_000;
    const CHANNELS: u8 = 2;
    const AMPLITUDE: i16 = 8_000;
    const CHUNK_FRAMES: usize = 480;

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

    fn write_all(client: &Client, handle: Handle, bytes: &[u8]) -> Result<(), Errno> {
        let mut offset = 0usize;
        while offset < bytes.len() {
            match client.write(handle, 0, &bytes[offset..]) {
                Ok(0) => return Err(Errno::EIO),
                Ok(count) => offset = offset.saturating_add(count),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn pcm(samples: &[i16]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(samples.len().saturating_mul(2));
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    fn run(options: &Options) -> Result<(), Errno> {
        let Some(mut tone) = Tone::new(Waveform::Sine, RATE, options.frequency_hz, AMPLITUDE)
        else {
            return Err(Errno::EINVAL);
        };
        let frames = usize::try_from(options.duration_ms)
            .map_err(|_| Errno::EINVAL)?
            .saturating_mul(48);
        let client = Client::new();
        let handle = client.open("audio:out0", 3)?;
        let format = Format {
            rate: RATE,
            channels: CHANNELS,
            sample: SampleFormat::S16Le,
        };
        if let Err(error) = expect_ok(&client, handle, Call::SetFormat { format }) {
            let _ = client.close(handle);
            return Err(error);
        }
        let _ = expect_ok(
            &client,
            handle,
            Call::SetVolume {
                left: 32_768,
                right: 32_768,
            },
        );
        expect_ok(&client, handle, Call::Start)?;
        let mut written = 0usize;
        while written < frames {
            let count = (frames - written).min(CHUNK_FRAMES);
            let mut samples = Vec::with_capacity(count.saturating_mul(2));
            for _ in 0..count {
                let sample = tone.next_sample();
                samples.push(sample);
                samples.push(sample);
            }
            write_all(&client, handle, &pcm(&samples))?;
            written = written.saturating_add(count);
        }
        expect_ok(&client, handle, Call::Drain)?;
        let _ = client.close(handle);
        raven_rt::println!("{}", summary(options));
        Ok(())
    }

    fn main(args: raven_rt::Args) -> i32 {
        let operands: Vec<String> = args
            .iter()
            .skip(1)
            .map(alloc::string::ToString::to_string)
            .collect();
        let options = match parse_args(&operands) {
            Ok(options) => options,
            Err(message) => {
                raven_rt::eprintln!("{message}");
                return 2;
            }
        };
        match run(&options) {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("beep: {}", error.description());
                1
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
