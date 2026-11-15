// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B44a `hw-sound` client: a 440 Hz tone through plain `WRITE`, then through the
//! `SET_BUFFER` shared ring. The QEMU `-audiodev wav` capture is checked by the qtest oracle.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::{Handle, Rights, map, prot};
    use raven_audio_core::tone::{Tone, Waveform};
    use raven_proto_audio::ring::{RingLayout, RingProducer};
    use raven_proto_audio::{
        Call, Format, SampleFormat, StreamRequest, StreamResponse, StreamState,
    };
    use raven_rt::entry;
    use raven_rt::spawn::{RtSys, Sys};
    use raven_scheme::Client;

    entry!(main);

    const RATE: u32 = 48_000;
    const CHANNELS: u8 = 2;
    const PERIOD_FRAMES: u32 = 480;
    const PERIOD_COUNT: u16 = 17;
    const RING_BYTES: usize = 32 * 1024;
    const TONE_HZ: u32 = 440;
    const AMPLITUDE: i16 = 8_000;
    const WRITE_FRAMES: u64 = 24_000;
    const RING_FRAMES: u64 = 144_000;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("hw-sound-client: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), &'static str> {
        let client = Client::new();
        write_path(&client)?;
        ring_path(&client)?;
        Ok(())
    }

    fn open(client: &Client) -> Result<Handle, Errno> {
        let mut last = Errno::ENOENT;
        for _ in 0..600 {
            match client.open("audio:out0", 3) {
                Ok(handle) => return Ok(handle),
                Err(error) => {
                    last = error;
                    let _ = raven_rt::nanosleep(100_000_000);
                }
            }
        }
        Err(last)
    }

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

    fn format() -> Format {
        Format {
            rate: RATE,
            channels: CHANNELS,
            sample: SampleFormat::S16Le,
        }
    }

    fn set_format_volume(client: &Client, handle: Handle) -> Result<(), Errno> {
        expect_ok(client, handle, Call::SetFormat { format: format() })?;
        expect_ok(
            client,
            handle,
            Call::SetVolume {
                left: 32_768,
                right: 32_768,
            },
        )
    }

    fn pcm(samples: &[i16]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(samples.len().saturating_mul(2));
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
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

    fn wait_position(client: &Client, handle: Handle, target: u64) -> Result<(u64, u32), Errno> {
        let mut last = (0u64, 0u32);
        for _ in 0..5000 {
            let (position, underruns, _) = status(client, handle)?;
            last = (position, underruns);
            if position >= target {
                return Ok(last);
            }
            let _ = raven_rt::nanosleep(2_000_000);
        }
        Ok(last)
    }

    fn write_path(client: &Client) -> Result<(), &'static str> {
        let handle = open(client).map_err(|_| "write path: no audio:out0")?;
        set_format_volume(client, handle).map_err(|_| "write path: format")?;

        let mut tone = Tone::new(Waveform::Sine, RATE, TONE_HZ, AMPLITUDE)
            .ok_or("write path: tone generator")?;
        let mut written = 0u64;
        while written < WRITE_FRAMES {
            let mut samples = [0i16; PERIOD_FRAMES as usize * 2];
            tone.fill_frames(&mut samples, CHANNELS);
            write_all(client, handle, &pcm(&samples)).map_err(|_| "write path: write")?;
            written = written.saturating_add(u64::from(PERIOD_FRAMES));
        }
        expect_ok(client, handle, Call::Start).map_err(|_| "write path: start")?;
        let (position, underruns) = wait_position(client, handle, WRITE_FRAMES - 4 * 480)
            .map_err(|_| "write path: status")?;
        let _ = expect_ok(client, handle, Call::Stop);
        let (final_position, final_underruns, _) =
            status(client, handle).map_err(|_| "write path: final status")?;
        let _ = client.close(handle);
        raven_rt::println!(
            "hw-sound: write ok frames={} position={} underruns={}",
            WRITE_FRAMES,
            final_position.max(position),
            final_underruns.max(underruns)
        );
        Ok(())
    }

    fn write_ring(producer: &mut RingProducer<'_>, bytes: &[u8]) -> Result<(), &'static str> {
        let frame_bytes = usize::from(producer.frame_bytes());
        let mut offset = 0usize;
        while offset < bytes.len() {
            match producer.write(&bytes[offset..]) {
                Ok(0) => {
                    let _ = raven_rt::nanosleep(2_000_000);
                }
                Ok(frames) => offset = offset.saturating_add(frames.saturating_mul(frame_bytes)),
                Err(_) => return Err("ring path: producer"),
            }
        }
        Ok(())
    }

    fn ring_path(client: &Client) -> Result<(), &'static str> {
        let handle = open(client).map_err(|_| "ring path: no audio:out0")?;
        set_format_volume(client, handle).map_err(|_| "ring path: format")?;

        let mut sys = RtSys;
        let object = sys
            .memobj_create(RING_BYTES)
            .map_err(|_| "ring path: memobj")?;
        let flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
        let address = sys
            .map(Handle::SELF_PROC, object, 0, 0, RING_BYTES, flags)
            .map_err(|_| "ring path: map")?;
        // SAFETY: `RtSys::map` just mapped a readable/writable shared object of RING_BYTES at
        // `address`; the process owns the mapping for its lifetime and no other reference aliases
        // it while `producer` is alive.
        let region = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, RING_BYTES) };
        let layout = RingLayout::new(RING_BYTES, format(), PERIOD_FRAMES, PERIOD_COUNT)
            .map_err(|_| "ring path: layout")?;
        layout
            .write_header(region, 0)
            .map_err(|_| "ring path: header")?;
        let mut producer = RingProducer::new(region, layout).map_err(|_| "ring path: producer")?;

        let rights = Rights::READ.union(Rights::WRITE).union(Rights::MAP).bits();
        let attached = client
            .transfer_to_server(handle, object, rights)
            .map_err(|_| "ring path: transfer")?;
        expect_ok(
            client,
            handle,
            Call::SetBuffer {
                handle: attached,
                period_frames: PERIOD_FRAMES,
                period_count: PERIOD_COUNT,
            },
        )
        .map_err(|_| "ring path: SET_BUFFER")?;
        raven_rt::println!("hw-sound: ring attached");

        let mut tone = Tone::new(Waveform::Sine, RATE, TONE_HZ, AMPLITUDE)
            .ok_or("ring path: tone generator")?;
        let mut written = 0u64;
        while written < 2 * u64::from(PERIOD_FRAMES) {
            let mut samples = [0i16; PERIOD_FRAMES as usize * 2];
            tone.fill_frames(&mut samples, CHANNELS);
            write_ring(&mut producer, &pcm(&samples))?;
            written = written.saturating_add(u64::from(PERIOD_FRAMES));
        }
        expect_ok(client, handle, Call::Start).map_err(|_| "ring path: start")?;
        raven_rt::println!("hw-sound: ring started");

        while written < RING_FRAMES {
            let mut samples = [0i16; PERIOD_FRAMES as usize * 2];
            tone.fill_frames(&mut samples, CHANNELS);
            write_ring(&mut producer, &pcm(&samples))?;
            written = written.saturating_add(u64::from(PERIOD_FRAMES));
        }
        let (position, underruns) =
            wait_position(client, handle, RING_FRAMES - 480).map_err(|_| "ring path: status")?;
        raven_rt::println!("hw-sound: ring drained position={position} underruns={underruns}");
        let _ = expect_ok(client, handle, Call::Stop);
        let (final_position, final_underruns, _) =
            status(client, handle).map_err(|_| "ring path: final status")?;
        let _ = client.close(handle);
        drop(producer);
        let _ = raven_rt::handle_close(object);
        raven_rt::println!(
            "hw-sound: ring ok frames={} position={} underruns={}",
            RING_FRAMES,
            final_position.max(position),
            final_underruns.max(underruns)
        );
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
