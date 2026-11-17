// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `talk` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::VecDeque;
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::types::Handle;
    use raven_proto_fs::{Errno, O_RDWR};
    use raven_proto_tty::{CALL_LEN, Mode, Request, Settings, encode_request};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::sync::Mutex;
    use raven_scheme::Client;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_USAGE, usage};
    use talk::{
        Event, Frame, FrameReader, Options, ProtocolError, Session, State, encode_frame,
        loopback_transcript, opposite_endpoint, parse_args, split_screen,
    };

    entry!(main);

    const TTYD_VERSION: u16 = 1;
    const OP_SESSION_ID: u16 = 0x26;
    const OP_PEER_LOOKUP: u16 = 0x30;
    const CONSOLE_STACK: usize = 16 * 1024;
    const WAIT_TICKS: usize = 6000;

    struct ConsoleInput {
        bytes: VecDeque<u8>,
        eof: bool,
        interrupt: bool,
    }

    impl ConsoleInput {
        const fn new() -> Self {
            Self {
                bytes: VecDeque::new(),
                eof: false,
                interrupt: false,
            }
        }
    }

    static INPUT: Mutex<ConsoleInput> = Mutex::new(ConsoleInput::new());

    struct Peer {
        tty: String,
        writable: bool,
    }

    fn main(args: raven_rt::Args) -> i32 {
        let args: Vec<String> = args.iter().skip(1).map(ToString::to_string).collect();
        if args.len() == 1 && args.first().is_some_and(|arg| arg == "--loopback") {
            return loopback();
        }
        let options = match parse_args(args) {
            Ok(options) => options,
            Err(_) => return usage_error(),
        };
        if options.listen {
            listener(&options)
        } else {
            caller(&options)
        }
    }

    fn loopback() -> i32 {
        match loopback_transcript() {
            Ok(lines) => {
                for line in lines {
                    raven_rt::println!("{line}");
                }
                0
            }
            Err(error) => {
                raven_rt::eprintln!("talk: loopback: {}", error.describe());
                EXIT_ERROR
            }
        }
    }

    fn caller(options: &Options) -> i32 {
        let Some(address) = &options.address else {
            return usage_error();
        };
        if let Some(host) = &address.host
            && host != "localhost"
        {
            raven_rt::eprintln!(
                "talk: network talk to {} is not implemented; only local tty-to-tty is supported",
                address.text()
            );
            return EXIT_ERROR;
        }
        let client = Client::new();
        let flags = match fs::s11_flags(O_RDWR) {
            Ok(flags) => flags,
            Err(error) => return fail("tty flags", error),
        };
        let (session_handle, session_id) = match open_session(&client, flags) {
            Ok(session) => session,
            Err(error) => return fail("tty session", error),
        };
        let own_tty = format!("pty:{session_id}/master");
        let registration = match register_peer(&client, flags, session_id, &address.user, &own_tty)
        {
            Ok(handle) => handle,
            Err(error) => {
                close(&client, session_handle);
                return fail("terminal registration", error);
            }
        };
        let peer = match lookup_peer(&client, flags, &address.user, options.terminal.as_deref()) {
            Ok(peer) => peer,
            Err(Errno::ENOENT) => {
                raven_rt::eprintln!("talk: {} is not logged in", address.user);
                close(&client, registration);
                close(&client, session_handle);
                return EXIT_ERROR;
            }
            Err(error) => {
                close(&client, registration);
                close(&client, session_handle);
                return fail("terminal registry", error);
            }
        };
        if !peer.writable {
            raven_rt::eprintln!("talk: {} has messages disabled", address.user);
            close(&client, registration);
            close(&client, session_handle);
            return EXIT_ERROR;
        }
        let Some(transport_path) = opposite_endpoint(&peer.tty) else {
            raven_rt::eprintln!("talk: peer terminal {} is not a pty endpoint", peer.tty);
            close(&client, registration);
            close(&client, session_handle);
            return EXIT_ERROR;
        };
        let transport = match client.open(&transport_path, flags) {
            Ok(handle) => handle,
            Err(error) => {
                raven_rt::eprintln!(
                    "talk: cannot open {transport_path}: {}",
                    error.description()
                );
                close(&client, registration);
                close(&client, session_handle);
                return EXIT_ERROR;
            }
        };
        set_raw(&client, transport);
        let console = match open_console() {
            Ok(handle) => handle,
            Err(error) => {
                close(&client, transport);
                close(&client, registration);
                close(&client, session_handle);
                return fail("console", error);
            }
        };
        let mut session = Session::caller(address.clone(), options.terminal.clone());
        let Event::Send(invite) = session.start() else {
            close(&client, console);
            close(&client, transport);
            close(&client, registration);
            close(&client, session_handle);
            return EXIT_ERROR;
        };
        if let Err(error) = send_frame(&client, transport, &invite) {
            raven_rt::eprintln!("talk: invitation failed: {}", error.description());
            close(&client, console);
            close(&client, transport);
            close(&client, registration);
            close(&client, session_handle);
            return EXIT_ERROR;
        }
        let mut reader = FrameReader::new();
        let reply = match wait_for(&client, transport, &mut reader) {
            Ok(frame) => frame,
            Err(message) => {
                raven_rt::eprintln!("talk: {message}");
                close(&client, console);
                close(&client, transport);
                close(&client, registration);
                close(&client, session_handle);
                return EXIT_ERROR;
            }
        };
        let code = match reply {
            Frame::Accept => run_session(&client, transport, &mut session, &mut reader),
            Frame::Decline { reason } => {
                raven_rt::eprintln!("talk: invitation declined: {reason}");
                EXIT_ERROR
            }
            _ => {
                raven_rt::eprintln!("talk: unexpected invitation reply");
                EXIT_ERROR
            }
        };
        close(&client, console);
        close(&client, transport);
        close(&client, registration);
        close(&client, session_handle);
        code
    }

    fn listener(options: &Options) -> i32 {
        let Some(address) = &options.address else {
            return usage_error();
        };
        let client = Client::new();
        let flags = match fs::s11_flags(O_RDWR) {
            Ok(flags) => flags,
            Err(error) => return fail("tty flags", error),
        };
        let (session_handle, session_id) = match open_session(&client, flags) {
            Ok(session) => session,
            Err(error) => return fail("tty session", error),
        };
        let tty = options
            .terminal
            .clone()
            .unwrap_or_else(|| format!("pty:{session_id}/slave"));
        let registration = match register_peer(&client, flags, session_id, &address.user, &tty) {
            Ok(handle) => handle,
            Err(error) => {
                close(&client, session_handle);
                return fail("terminal registration", error);
            }
        };
        let transport = match client.open(&tty, flags) {
            Ok(handle) => handle,
            Err(error) => {
                raven_rt::eprintln!("talk: cannot open {tty}: {}", error.description());
                close(&client, registration);
                close(&client, session_handle);
                return EXIT_ERROR;
            }
        };
        set_raw(&client, transport);
        let console = match open_console() {
            Ok(handle) => handle,
            Err(error) => {
                close(&client, transport);
                close(&client, registration);
                close(&client, session_handle);
                return fail("console", error);
            }
        };
        raven_rt::println!("talk: listening on {tty} for {}", address.user);
        let mut reader = FrameReader::new();
        let invite = match wait_for(&client, transport, &mut reader) {
            Ok(frame) => frame,
            Err(message) => {
                raven_rt::eprintln!("talk: {message}");
                close(&client, console);
                close(&client, transport);
                close(&client, registration);
                close(&client, session_handle);
                return EXIT_ERROR;
            }
        };
        let mut session = Session::listener();
        match session.receive(invite) {
            Event::ShowInvitation(text) => raven_rt::print!("{text}"),
            _ => {
                raven_rt::eprintln!("talk: unexpected invitation");
                close(&client, console);
                close(&client, transport);
                close(&client, registration);
                close(&client, session_handle);
                return EXIT_ERROR;
            }
        }
        let code = if ask_accept() {
            let Event::Send(accept) = session.accept() else {
                close(&client, console);
                close(&client, transport);
                close(&client, registration);
                close(&client, session_handle);
                return EXIT_ERROR;
            };
            if let Err(error) = send_frame(&client, transport, &accept) {
                raven_rt::eprintln!("talk: accept failed: {}", error.description());
                EXIT_ERROR
            } else {
                run_session(&client, transport, &mut session, &mut reader)
            }
        } else {
            let Event::Send(decline) = session.decline("declined") else {
                close(&client, console);
                close(&client, transport);
                close(&client, registration);
                close(&client, session_handle);
                return EXIT_ERROR;
            };
            let _ = send_frame(&client, transport, &decline);
            raven_rt::eprintln!("talk: invitation declined");
            EXIT_ERROR
        };
        close(&client, console);
        close(&client, transport);
        close(&client, registration);
        close(&client, session_handle);
        code
    }

    fn run_session(
        client: &Client,
        transport: Handle,
        session: &mut Session,
        reader: &mut FrameReader,
    ) -> i32 {
        raven_rt::print!("{}", split_screen("you", "peer"));
        loop {
            if let Some(frame) = read_frame(client, transport, reader) {
                match frame {
                    Ok(frame) => {
                        let event = session.receive(frame);
                        if dispatch(event, client, transport) {
                            return session.exit_status();
                        }
                    }
                    Err(_) => return EXIT_ERROR,
                }
            }
            let (bytes, eof, interrupt) = take_input();
            if interrupt && dispatch(session.escape(), client, transport) {
                return session.exit_status();
            }
            if eof && dispatch(session.local_eof(), client, transport) {
                return session.exit_status();
            }
            for byte in bytes {
                let event = session.local_char(char::from(byte));
                if dispatch(event, client, transport) {
                    return session.exit_status();
                }
            }
            if session.state() == State::Closed {
                return session.exit_status();
            }
            let _ = raven_rt::nanosleep(1_000_000);
        }
    }

    fn dispatch(event: Event, client: &Client, transport: Handle) -> bool {
        match event {
            Event::Send(frame) => {
                let _ = send_frame(client, transport, &frame);
                false
            }
            Event::Show(text) => {
                raven_rt::print!("{text}");
                false
            }
            Event::Alert => {
                raven_rt::print!("\x07");
                false
            }
            Event::Refresh => {
                raven_rt::print!("{}", talk::refresh_screen());
                false
            }
            Event::RemoteEnded => {
                raven_rt::println!();
                raven_rt::println!("talk: peer ended; press Ctrl-C to exit");
                false
            }
            Event::Closed => true,
            Event::Start | Event::ShowInvitation(_) | Event::Declined(_) | Event::Ignore => false,
        }
    }

    fn open_session(client: &Client, flags: u32) -> Result<(Handle, u32), Errno> {
        let handle = client.open("ttyd:session", flags)?;
        let mut request = [0u8; CALL_LEN];
        request[..2].copy_from_slice(&TTYD_VERSION.to_le_bytes());
        request[2..4].copy_from_slice(&OP_SESSION_ID.to_le_bytes());
        let mut response = [0u8; CALL_LEN];
        if let Err(error) = client.call(handle, &request, &mut response) {
            let _ = client.close(handle);
            return Err(error);
        }
        let session = u32::from_le_bytes([response[4], response[5], response[6], response[7]]);
        Ok((handle, session))
    }

    fn register_peer(
        client: &Client,
        flags: u32,
        session: u32,
        user: &str,
        tty: &str,
    ) -> Result<Handle, Errno> {
        client.open(&format!("ttyd:peer/{session}/{user}/{tty}"), flags)
    }

    fn lookup_peer(
        client: &Client,
        flags: u32,
        user: &str,
        terminal: Option<&str>,
    ) -> Result<Peer, Errno> {
        let path = match terminal {
            Some(tty) => format!("ttyd:who/{user}/{tty}"),
            None => format!("ttyd:who/{user}"),
        };
        let handle = client.open(&path, flags)?;
        let mut request = [0u8; CALL_LEN];
        request[..2].copy_from_slice(&TTYD_VERSION.to_le_bytes());
        request[2..4].copy_from_slice(&OP_PEER_LOOKUP.to_le_bytes());
        let mut response = [0u8; 64];
        let result = client.call(handle, &request, &mut response);
        let _ = client.close(handle);
        let length = result?;
        if length < CALL_LEN {
            return Err(Errno::EPROTO);
        }
        let writable =
            u32::from_le_bytes([response[4], response[5], response[6], response[7]]) != 0;
        let tty_length = usize::try_from(u32::from_le_bytes([
            response[8],
            response[9],
            response[10],
            response[11],
        ]))
        .map_err(|_| Errno::EPROTO)?;
        let end = CALL_LEN.checked_add(tty_length).ok_or(Errno::EPROTO)?;
        if end > length || end > response.len() {
            return Err(Errno::EPROTO);
        }
        let tty = core::str::from_utf8(&response[CALL_LEN..end]).map_err(|_| Errno::EPROTO)?;
        Ok(Peer {
            tty: tty.to_string(),
            writable,
        })
    }

    fn open_console() -> Result<Handle, Errno> {
        let flags = fs::s11_flags(O_RDWR)?;
        let handle = Client::new().open("tty:console", flags)?;
        raven_rt::thread::spawn(reader, handle.0 as usize, CONSOLE_STACK)?;
        Ok(handle)
    }

    fn reader(argument: usize) {
        let Ok(raw) = u32::try_from(argument) else {
            return;
        };
        let handle = Handle(raw);
        let client = Client::new();
        let mut buffer = [0u8; 128];
        loop {
            match client.read(handle, 0, &mut buffer) {
                Ok(0) => {
                    INPUT.lock().eof = true;
                    return;
                }
                Ok(count) => {
                    let mut input = INPUT.lock();
                    for byte in &buffer[..count] {
                        input.bytes.push_back(*byte);
                    }
                }
                Err(Errno::EINTR) => {
                    INPUT.lock().interrupt = true;
                }
                Err(_) => {
                    let _ = raven_rt::nanosleep(10_000_000);
                }
            }
        }
    }

    fn take_input() -> (Vec<u8>, bool, bool) {
        let mut input = INPUT.lock();
        let bytes: Vec<u8> = input.bytes.drain(..).collect();
        let eof = input.eof;
        let interrupt = input.interrupt;
        input.eof = false;
        input.interrupt = false;
        (bytes, eof, interrupt)
    }

    fn ask_accept() -> bool {
        raven_rt::print!("talk: accept? [y/n] ");
        loop {
            let (bytes, eof, interrupt) = take_input();
            if eof || interrupt {
                return false;
            }
            for byte in bytes {
                match byte {
                    b'y' | b'Y' => return true,
                    b'n' | b'N' => return false,
                    _ => {}
                }
            }
            let _ = raven_rt::nanosleep(10_000_000);
        }
    }

    fn wait_for(
        client: &Client,
        transport: Handle,
        reader: &mut FrameReader,
    ) -> Result<Frame, &'static str> {
        let mut ticks = 0usize;
        loop {
            if let Some(frame) = read_frame(client, transport, reader) {
                return frame.map_err(|_| "protocol error");
            }
            let (_, eof, interrupt) = take_input();
            if eof || interrupt {
                return Err("interrupted");
            }
            ticks = ticks.saturating_add(1);
            if ticks >= WAIT_TICKS {
                return Err("no reply");
            }
            let _ = raven_rt::nanosleep(10_000_000);
        }
    }

    fn read_frame(
        client: &Client,
        transport: Handle,
        reader: &mut FrameReader,
    ) -> Option<Result<Frame, ProtocolError>> {
        if let Some(frame) = reader.next_frame() {
            return Some(frame);
        }
        let mut buffer = [0u8; 64];
        match client.read(transport, 0, &mut buffer) {
            Ok(0) => Some(Ok(Frame::Bye)),
            Ok(count) => {
                if let Err(error) = reader.push(&buffer[..count]) {
                    return Some(Err(error));
                }
                reader.next_frame()
            }
            Err(Errno::EAGAIN) => None,
            Err(_) => None,
        }
    }

    fn send_frame(client: &Client, transport: Handle, frame: &Frame) -> Result<(), Errno> {
        let mut line = encode_frame(frame).map_err(|_| Errno::EINVAL)?;
        line.push('\n');
        let bytes = line.as_bytes();
        let mut offset = 0usize;
        while offset < bytes.len() {
            let written = client.write(transport, 0, &bytes[offset..])?;
            if written == 0 {
                return Err(Errno::EIO);
            }
            offset = offset.saturating_add(written);
        }
        Ok(())
    }

    fn set_raw(client: &Client, handle: Handle) {
        let request = Request::SetMode {
            settings: Settings {
                mode: Mode::Raw,
                echo: false,
            },
        };
        let mut body = [0u8; CALL_LEN];
        encode_request(request, &mut body);
        let mut response = [0u8; CALL_LEN];
        let _ = client.call(handle, &body, &mut response);
    }

    fn close(client: &Client, handle: Handle) {
        let _ = client.close(handle);
    }

    fn usage_error() -> i32 {
        raven_rt::eprintln!("{}", usage("talk", "[-l] address [terminal]"));
        EXIT_USAGE
    }

    fn fail(what: &str, error: Errno) -> i32 {
        raven_rt::eprintln!("talk: {what}: {}", error.description());
        EXIT_ERROR
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(host_run(&args));
}

#[cfg(not(target_os = "none"))]
fn host_run(args: &[String]) -> i32 {
    if args.len() == 1 && args.first().is_some_and(|arg| arg == "--loopback") {
        return match talk::loopback_transcript() {
            Ok(lines) => {
                for line in lines {
                    println!("{line}");
                }
                0
            }
            Err(error) => {
                eprintln!("talk: loopback: {}", error.describe());
                1
            }
        };
    }
    let options = match talk::parse_args(args.iter().cloned()) {
        Ok(options) => options,
        Err(_) => {
            eprintln!("usage: {}", talk::USAGE);
            return 2;
        }
    };
    if options.listen {
        eprintln!("talk: no peer transport for a local listener");
        return 1;
    }
    let Some(address) = &options.address else {
        eprintln!("usage: {}", talk::USAGE);
        return 2;
    };
    if let Some(host) = &address.host
        && host != "localhost"
    {
        eprintln!(
            "talk: network talk to {} is not implemented; only local tty-to-tty is supported",
            address.text()
        );
        return 1;
    }
    eprintln!(
        "talk: no terminal registry entry for {}; {} is not reachable",
        address.user, address.user
    );
    1
}
