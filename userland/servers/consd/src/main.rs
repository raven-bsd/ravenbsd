// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD console server (`tty:` scheme).
//!
//! `tty:console` is served from a [`Multiplexer`]: UART is the default input/output, a keyboard
//! source (`kbd:console`) is added when present, and output is fanned out to UART plus an optional
//! framebuffer text sink (`fb:console`). The line discipline and the scheme contract are
//! unchanged; getty/login still open `tty:console`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use core::sync::atomic::{AtomicU32, Ordering};

    use raven_abi::errno::Errno;
    use raven_consd::{ConsoleInput, ConsoleOutput, InputError, Multiplexer, OutputError};
    use raven_proto_fs::{O_RDONLY, O_WRONLY, S_IFCHR, Stat};
    use raven_proto_tty::{
        CALL_LEN, OP_GET_WRITE, OP_SET_WRITE, Request, Response, WriteRequest, decode_request,
        decode_write_request, encode_response, encode_write_response,
    };
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::sync::Mutex;
    use raven_scheme::{CallReply, Ctx, Handler, HandlerError, HandlerResult, Opened, Server};

    entry!(main);

    /// Console path accepted by `OPEN`.
    const CONSOLE_PATH: &str = "console";
    /// Bytes read from a backend per call.
    const READ_CHUNK: usize = 256;
    /// Consecutive UART backend I/O errors tolerated before the console is closed.
    const IO_RETRY_LIMIT: u32 = 50;
    /// Backoff between UART backend I/O retries.
    const IO_RETRY_DELAY_NS: u64 = 10_000_000;
    /// Reader thread stack.
    const READER_STACK: usize = 32 * 1024;

    const OPEN_READ: u32 = 1;
    const OPEN_WRITE: u32 = 2;

    /// Wakes a blocked `READ` whenever the console state changes.
    static SEQ: AtomicU32 = AtomicU32::new(0);

    static CONSOLE: Mutex<Multiplexer> = Mutex::new(Multiplexer::new());

    /// Publish a state change: bump the sequence word and wake blocked readers.
    fn publish() {
        SEQ.fetch_add(1, Ordering::Release);
        let _ = raven_rt::futex_wake(&SEQ, u32::MAX);
    }

    /// UART debug input.
    struct UartInput;

    impl ConsoleInput for UartInput {
        fn read(&mut self, out: &mut [u8]) -> Result<usize, InputError> {
            match raven_rt::debug_read(out, 0) {
                Ok(count) => Ok(count),
                Err(Errno::EINTR) => Err(InputError::WouldBlock),
                Err(_) => Err(InputError::Io),
            }
        }
    }

    /// UART debug output.
    struct UartOutput;

    impl ConsoleOutput for UartOutput {
        fn write(&mut self, bytes: &[u8]) -> Result<usize, OutputError> {
            match raven_rt::debug_write(bytes) {
                Ok(0) => Err(OutputError::Full),
                Ok(count) => Ok(count),
                Err(_) => Err(OutputError::Closed),
            }
        }
    }

    /// A byte sink backed by a scheme (`fb:console`), owned by HDMI1/UI1.
    struct SchemeOutput {
        file: fs::File,
    }

    impl ConsoleOutput for SchemeOutput {
        fn write(&mut self, bytes: &[u8]) -> Result<usize, OutputError> {
            match self.file.write(bytes) {
                Ok(0) => Err(OutputError::Full),
                Ok(count) => Ok(count),
                Err(_) => Err(OutputError::Closed),
            }
        }
    }

    /// A byte source backed by a scheme (`kbd:console`), owned by USB1.
    struct SchemeInput {
        file: fs::File,
    }

    impl ConsoleInput for SchemeInput {
        fn read(&mut self, out: &mut [u8]) -> Result<usize, InputError> {
            match self.file.read(out) {
                Ok(0) => Err(InputError::EndOfFile),
                Ok(count) => Ok(count),
                Err(Errno::EINTR) => Err(InputError::WouldBlock),
                Err(_) => Err(InputError::Io),
            }
        }
    }

    /// UART reader thread: pull bytes and feed the multiplexer forever.
    fn uart_reader(_argument: usize) {
        let mut input = UartInput;
        let mut buffer = [0u8; READ_CHUNK];
        let mut io_retries = 0u32;
        loop {
            match input.read(&mut buffer) {
                Ok(0) => {
                    io_retries = 0;
                }
                Ok(count) => {
                    io_retries = 0;
                    let count = count.min(buffer.len());
                    let changed = CONSOLE.lock().feed(&buffer[..count]);
                    if changed {
                        publish();
                    }
                }
                Err(InputError::WouldBlock) => {
                    io_retries = 0;
                }
                // A real EOF or closed backend ends the console.
                Err(InputError::EndOfFile | InputError::Closed) => {
                    CONSOLE.lock().mark_eof();
                    publish();
                    break;
                }
                // A transient backend error must not look like a live console forever: retry
                // briefly, then report the persistent failure and close the console.
                Err(InputError::Io) => {
                    io_retries = io_retries.saturating_add(1);
                    if io_retries > IO_RETRY_LIMIT {
                        raven_rt::eprintln!(
                            "consd: uart input: {io_retries} consecutive I/O errors; closing console"
                        );
                        CONSOLE.lock().mark_eof();
                        publish();
                        break;
                    }
                    let _ = raven_rt::nanosleep(IO_RETRY_DELAY_NS);
                }
            }
        }
    }

    /// Keyboard reader thread: opens `kbd:console` and disappears if USB1 is absent.
    fn keyboard_reader(_argument: usize) {
        let Ok(mut input) = fs::open("kbd:console", O_RDONLY).map(|file| SchemeInput { file })
        else {
            return;
        };
        let mut buffer = [0u8; READ_CHUNK];
        loop {
            match input.read(&mut buffer) {
                Ok(count) => {
                    let changed = CONSOLE.lock().feed(&buffer[..count]);
                    if changed {
                        publish();
                    }
                }
                Err(InputError::WouldBlock) => continue,
                Err(InputError::EndOfFile | InputError::Closed | InputError::Io) => break,
            }
        }
    }

    struct OpenFile {
        readable: bool,
        writable: bool,
    }

    struct ConsoleHandler {
        files: BTreeMap<u64, OpenFile>,
        next: u64,
    }

    impl ConsoleHandler {
        fn new() -> Self {
            Self {
                files: BTreeMap::new(),
                next: 1,
            }
        }

        fn file(&self, file: u64) -> Result<&OpenFile, Errno> {
            self.files.get(&file).ok_or(Errno::EBADF)
        }
    }

    impl Handler for ConsoleHandler {
        fn open(
            &mut self,
            path: &str,
            flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            // The scheme root (`tty:` or `tty:/`) is the console: readiness probes open it.
            let path = path.strip_prefix('/').unwrap_or(path);
            if !path.is_empty() && path != CONSOLE_PATH {
                return Err(Errno::ENOENT.into());
            }
            let readable = flags & OPEN_READ != 0;
            let writable = flags & OPEN_WRITE != 0;
            if !readable && !writable {
                return Err(Errno::EACCES.into());
            }
            let file = self.next;
            self.next = self.next.wrapping_add(1);
            self.files.insert(file, OpenFile { readable, writable });
            Ok(Opened::File(file))
        }

        fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            if !self.file(file)?.readable {
                return Err(Errno::EACCES.into());
            }
            if buffer.is_empty() {
                return Ok(0);
            }
            // Never sleep inside the single-threaded scheme server: a blocked `read` keeps the
            // server from dispatching queued `write` requests (the console submit queue fills and
            // the session output stalls). A caller that wants blocking semantics retries on
            // `EAGAIN`; the input thread publishes every state change.
            {
                let mut console = CONSOLE.lock();
                if console.take_interrupt() {
                    return Err(Errno::EINTR.into());
                }
                if console.has_ready() {
                    return Ok(console.take(buffer));
                }
                if console.eof() {
                    return Ok(0);
                }
            }
            Err(Errno::EAGAIN.into())
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            if !self.file(file)?.writable {
                return Err(Errno::EACCES.into());
            }
            match CONSOLE.lock().write_all(buffer) {
                Ok(count) => Ok(count),
                Err(OutputError::Full) => Err(Errno::EAGAIN.into()),
                Err(OutputError::Closed | OutputError::Io) => Err(Errno::EIO.into()),
            }
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.files
                .remove(&file)
                .map(|_| ())
                .ok_or(Errno::EBADF.into())
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            let (readable, writable) = {
                let open = self.file(file)?;
                (open.readable, open.writable)
            };
            let duplicate = self.next;
            self.next = self.next.wrapping_add(1);
            self.files
                .insert(duplicate, OpenFile { readable, writable });
            Ok(duplicate)
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            self.file(file)?;
            let allowed = CONSOLE.lock().write_allowed();
            let mode = if allowed { 0o620 } else { 0o600 };
            Ok(Stat {
                ino: file,
                mode: S_IFCHR | mode,
                nlink: 1,
                ..Stat::default()
            })
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            self.file(file)?;
            if response.len() < CALL_LEN {
                return Err(Errno::EINVAL.into());
            }
            if request.len() >= CALL_LEN {
                let op = u16::from_le_bytes([request[2], request[3]]);
                if op == OP_GET_WRITE || op == OP_SET_WRITE {
                    let allowed = match decode_write_request(request)
                        .map_err(|_| HandlerError::from(Errno::EINVAL))?
                    {
                        WriteRequest::Get => CONSOLE.lock().write_allowed(),
                        WriteRequest::Set { allowed } => {
                            CONSOLE.lock().set_write_allowed(allowed);
                            allowed
                        }
                    };
                    let mut encoded = [0u8; CALL_LEN];
                    encode_write_response(allowed, &mut encoded);
                    response[..CALL_LEN].copy_from_slice(&encoded);
                    return Ok(CALL_LEN);
                }
            }
            let request = decode_request(request).map_err(|_| HandlerError::from(Errno::ENOSYS))?;
            let reply = match request {
                Request::GetMode => Response::Mode(CONSOLE.lock().settings()),
                Request::SetMode { settings } => {
                    let applied = {
                        let mut console = CONSOLE.lock();
                        let _ = console.set_settings(settings);
                        console.settings()
                    };
                    publish();
                    Response::Mode(applied)
                }
                Request::Flush => {
                    let bytes = CONSOLE.lock().flush();
                    publish();
                    Response::Flushed { bytes }
                }
            };
            let mut encoded = [0u8; CALL_LEN];
            encode_response(reply, &mut encoded);
            response[..CALL_LEN].copy_from_slice(&encoded);
            Ok(CALL_LEN)
        }

        fn call_with(
            &mut self,
            file: u64,
            request: &[u8],
            response: &mut [u8],
            ctx: &Ctx,
        ) -> HandlerResult<CallReply> {
            if request.len() >= 4 {
                let op = u16::from_le_bytes([request[2], request[3]]);
                if op == OP_SET_WRITE && ctx.credentials.euid != 0 {
                    return Err(Errno::EPERM.into());
                }
            }
            Ok(CallReply::new(self.call(file, request, response)?))
        }

        fn write_with(
            &mut self,
            file: u64,
            offset: u64,
            buffer: &[u8],
            ctx: &Ctx,
        ) -> HandlerResult<usize> {
            if !CONSOLE.lock().write_allowed() && ctx.credentials.euid != 0 {
                return Err(Errno::EACCES.into());
            }
            self.write(file, offset, buffer)
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let mut server = match Server::register("tty") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("consd: {}", error.description());
                return 1;
            }
        };
        {
            let mut console = CONSOLE.lock();
            console.add_output(Box::new(UartOutput));
        }
        // HDMI1/UI1 may publish a text framebuffer sink; absence keeps UART-only behaviour.
        if let Ok(file) = fs::open("fb:console", O_WRONLY) {
            CONSOLE.lock().add_output(Box::new(SchemeOutput { file }));
        }
        if let Err(error) = raven_rt::thread::spawn(uart_reader, 0, READER_STACK) {
            raven_rt::eprintln!("consd: uart reader thread: {}", error.description());
            return 1;
        }
        // USB1 may publish kbd:console; the thread exits immediately when it is absent.
        let _ = raven_rt::thread::spawn(keyboard_reader, 0, READER_STACK);
        let mut handler = ConsoleHandler::new();
        loop {
            let Some(packet) = server.poll(None) else {
                continue;
            };
            let _ = server.dispatch(packet, &mut handler);
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
