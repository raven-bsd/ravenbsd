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
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use audiod::mixer::{Mixer, PERIOD_FRAMES};
    use raven_abi::errno::Errno;
    use raven_proto_audio::{StreamRequest, StreamResponse};
    use raven_proto_fs::{S_IFCHR, Stat};
    use raven_rt::entry;
    use raven_rt::sync::Mutex;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    /// The only device served in the first gate.
    const DEVICE: &str = "out0";

    static MIXER: Mutex<Option<Mixer>> = Mutex::new(None);

    fn with_mixer<R>(function: impl FnOnce(&mut Mixer) -> R) -> R {
        let mut guard = MIXER.lock();
        let mixer = guard.get_or_insert_with(Mixer::new);
        function(mixer)
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
                mixer.pump();
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
                    mixer.pump();
                    Ok::<StreamResponse, Errno>(StreamResponse::Ok)
                })
                .unwrap_or_else(StreamResponse::Err),
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

    fn main(_args: raven_rt::Args) -> i32 {
        let mut server = match Server::register("audio") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("audiod: {}", error.description());
                return 1;
            }
        };
        let _ = with_mixer(|mixer| mixer.pump());
        raven_rt::println!("audiod: ready (null sink, {PERIOD_FRAMES} frames/period)");
        let mut handler = AudioHandler;
        server.run(&mut handler);
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
