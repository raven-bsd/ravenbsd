// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD audio server (`audio:` scheme).
//!
//! `audio:out0` is a null sink: the mixer consumes PCM through plain `WRITE` (or the protocol's
//! `WRITE` call) and the server exposes the `CALL` operations from `raven-proto-audio`. The null
//! sink pulls a period only when every running stream has one, so a cooperative client pair is
//! deterministic and never underruns. The driver ring (`SET_BUFFER`) is the HDA path and is
//! reported `ENOTSUP` here.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use audiod::mixer::{DeviceSink, Mixer, PERIOD_FRAMES};
    use raven_abi::errno::Errno;
    use raven_abi::types::{Handle, map, prot};
    use raven_proto_audio::ring::{RingHeader, RingLayout, RingProducer};
    use raven_proto_audio::{Call, StreamRequest, StreamResponse, StreamState};
    use raven_proto_fs::{S_IFCHR, Stat};
    use raven_rt::spawn::{RtSys, Sys};
    use raven_rt::sync::Mutex;
    use raven_rt::{entry, map_mmio_mut, thread};
    use raven_scheme::{Client, Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    /// The only device served in the first gate.
    const DEVICE: &str = "out0";
    /// The driver ring mapping used by the HDA sink (the driver initialises the header).
    const DRIVER_RING_BYTES: usize = 32 * 1024;
    /// One device period at 48 kHz; the pump thread uses this to pace the sink.
    const DEVICE_PERIOD_NS: u64 = 10_000_000;

    static MIXER: Mutex<Option<Mixer>> = Mutex::new(None);
    static SINK: Mutex<Option<HdaSink>> = Mutex::new(None);
    static SINK_PATH: Mutex<Option<alloc::string::String>> = Mutex::new(None);

    struct HdaSink {
        producer: RingProducer<'static>,
    }

    impl HdaSink {
        fn open(path: &str) -> Result<Self, Errno> {
            let client = Client::new();
            let handle = client.open(path, 3)?;
            let region = map_mmio_mut(handle, DRIVER_RING_BYTES)?;
            let header = RingHeader::read_from(region).map_err(|_| Errno::EINVAL)?;
            let layout =
                RingLayout::from_header(&header, region.len()).map_err(|_| Errno::EINVAL)?;
            let format = header.format();
            if format.rate != 48_000
                || format.channels != 2
                || format.sample != raven_proto_audio::SampleFormat::S16Le
                || header.period_frames != PERIOD_FRAMES
                || header.period_count != 17
            {
                return Err(Errno::EOPNOTSUPP);
            }
            let producer = RingProducer::new(region, layout).map_err(|_| Errno::EINVAL)?;
            Ok(Self { producer })
        }
    }

    impl Drop for HdaSink {
        fn drop(&mut self) {
            self.producer.stop();
        }
    }

    impl DeviceSink for HdaSink {
        fn write(&mut self, period: &[i16]) -> Result<(), Errno> {
            let mut bytes = alloc::vec::Vec::with_capacity(period.len().saturating_mul(2));
            for sample in period {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            if self.producer.state() == StreamState::Stopped {
                self.producer.start();
            }
            match self.producer.write(&bytes) {
                Ok(frames) if frames == period.len() / 2 => Ok(()),
                _ => Err(Errno::EIO),
            }
        }

        fn position_frames(&self) -> u64 {
            self.producer.position_frames()
        }

        fn underruns(&self) -> u32 {
            self.producer.underruns()
        }

        fn idle(&mut self) {
            if self.producer.state() != StreamState::Stopped {
                self.producer.stop();
                raven_rt::println!("audiod: sink idle");
            }
        }
    }

    fn pump(_argument: usize) {
        loop {
            if raven_rt::nanosleep(DEVICE_PERIOD_NS).is_err() {
                return;
            }
            let mut sink = SINK.lock();
            if sink.is_none() {
                let path = {
                    let guard = SINK_PATH.lock();
                    guard.as_ref().cloned()
                };
                let Some(path) = path else {
                    continue;
                };
                match HdaSink::open(&path) {
                    Ok(device) => {
                        *sink = Some(device);
                        with_mixer(|mixer| mixer.set_auto_pump(false));
                        raven_rt::println!("audiod: hda sink connected");
                    }
                    Err(_) => continue,
                }
            }
            let Some(device) = sink.as_mut() else {
                continue;
            };
            let result = with_mixer(|mixer| mixer.pump_device(device));
            match result {
                Ok(0) => device.idle(),
                Ok(_) => {}
                Err(_) => {
                    let _ = sink.take();
                    with_mixer(|mixer| mixer.set_auto_pump(false));
                    raven_rt::eprintln!("audiod: hda sink stopped");
                }
            }
        }
    }

    fn with_mixer<R>(function: impl FnOnce(&mut Mixer) -> R) -> R {
        let mut guard = MIXER.lock();
        let mixer = guard.get_or_insert_with(Mixer::new);
        function(mixer)
    }

    /// Map a client memory object with `MAP_SHARED` so both sides see the ring indices.
    fn map_shared(handle: Handle, len: usize) -> Result<&'static mut [u8], Errno> {
        if len == 0 || !len.is_multiple_of(4096) {
            return Err(Errno::EINVAL);
        }
        let mut sys = RtSys;
        let flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
        let address = sys.map(Handle::SELF_PROC, handle, 0, 0, len, flags)?;
        // SAFETY: `MAP_SHARED` mapped a readable/writable region of `len` bytes at `address` in
        // this process; the mapping lives until process exit and this is the only alias the
        // server creates for the client ring.
        Ok(unsafe { core::slice::from_raw_parts_mut(address as *mut u8, len) })
    }

    struct AudioHandler;

    impl Handler for AudioHandler {
        fn open(
            &mut self,
            path: &str,
            _flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            if path.is_empty() {
                // svcd's scheme readiness probe opens `audio:` with no path.
                return Ok(Opened::File(0));
            }
            if path != DEVICE {
                return Err(Errno::ENOENT.into());
            }
            let id = with_mixer(Mixer::open);
            Ok(Opened::File(id))
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            with_mixer(|mixer| {
                let written = mixer.write(file, buffer)?;
                mixer.pump_if_null();
                Ok(written)
            })
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            let decoded = match StreamRequest::decode(request) {
                Ok(decoded) => decoded,
                Err(_) => {
                    return reply(response, StreamResponse::Err(Errno::EINVAL));
                }
            };
            let answer = match decoded {
                StreamRequest::Open { .. } => StreamResponse::Err(Errno::EINVAL),
                StreamRequest::Write { data } => with_mixer(|mixer| {
                    let _ = mixer.write(file, data)?;
                    mixer.pump_if_null();
                    Ok::<StreamResponse, Errno>(StreamResponse::Ok)
                })
                .unwrap_or_else(StreamResponse::Err),
                StreamRequest::Call(Call::SetBuffer {
                    handle,
                    period_frames,
                    period_count,
                }) => {
                    let result = (|| -> Result<(), Errno> {
                        let format =
                            with_mixer(|mixer| mixer.stream(file).map(|stream| stream.format))?;
                        let frame_bytes = usize::from(format.bytes_per_frame());
                        let data = usize::try_from(period_frames)
                            .map_err(|_| Errno::EINVAL)?
                            .saturating_mul(usize::from(period_count))
                            .saturating_mul(frame_bytes);
                        let data_offset = (raven_proto_audio::ring::HEADER_LEN
                            .div_ceil(raven_proto_audio::ring::DATA_ALIGN))
                            * raven_proto_audio::ring::DATA_ALIGN;
                        let region = map_shared(handle, data_offset.saturating_add(data))?;
                        let result = with_mixer(|mixer| {
                            mixer.set_buffer(file, region, period_frames, period_count)
                        });
                        let _ = raven_rt::handle_close(handle);
                        result
                    })();
                    match result {
                        Ok(()) => StreamResponse::Ok,
                        Err(error) => StreamResponse::Err(error),
                    }
                }
                StreamRequest::Call(call) => with_mixer(|mixer| match mixer.call(file, call) {
                    Ok(Some(status)) => Ok(StreamResponse::Status(status)),
                    Ok(None) => Ok(StreamResponse::Ok),
                    Err(error) => Err(error),
                })
                .unwrap_or_else(StreamResponse::Err),
            };
            reply(response, answer)
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            if file == 0 {
                return Ok(());
            }
            with_mixer(|mixer| mixer.close(file)).map_err(Into::into)
        }

        fn fstat(&mut self, _file: u64) -> HandlerResult<Stat> {
            Ok(Stat {
                mode: S_IFCHR | 0o666,
                nlink: 1,
                ..Stat::default()
            })
        }
    }

    fn reply(response: &mut [u8], answer: StreamResponse) -> HandlerResult<usize> {
        match answer.encode(response) {
            Ok(length) => Ok(length),
            Err(_) => Err(Errno::EPROTO.into()),
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let mut server = match Server::register("audio") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("audiod: {}", error.description());
                return 1;
            }
        };
        let _ = with_mixer(|mixer| mixer.pump_if_null());
        let mut sink_name = "null sink";
        if let Some(path) = args.env_get("RAVEN_AUDIO_SINK") {
            *SINK_PATH.lock() = Some(alloc::string::String::from(path));
            with_mixer(|mixer| mixer.set_auto_pump(false));
            sink_name = "hda sink";
            match HdaSink::open(path) {
                Ok(sink) => {
                    *SINK.lock() = Some(sink);
                    raven_rt::println!("audiod: hda sink connected");
                }
                Err(error) => {
                    raven_rt::eprintln!(
                        "audiod: hda sink unavailable: {} (retrying)",
                        error.description()
                    );
                }
            }
            if let Err(error) = thread::spawn(pump, 0, 64 * 1024) {
                raven_rt::eprintln!("audiod: pump thread: {}", error.description());
                let _ = SINK.lock().take();
                with_mixer(|mixer| mixer.set_auto_pump(true));
                sink_name = "null sink";
            }
        }
        raven_rt::println!("audiod: ready ({sink_name}, {PERIOD_FRAMES} frames/period)");
        let mut handler = AudioHandler;
        server.run(&mut handler);
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
