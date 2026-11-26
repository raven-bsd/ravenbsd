// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `rsh`, the RavenBSD shell (K37).
//!
//! The shell owns the console through `tty:console` (K36), parses a line with `raven-shell-core`,
//! runs the builtins itself and spawns programs from the initfs (`raven_rt::spawn`), waiting for
//! each child and keeping `$?`. It keeps its own working directory and passes it to children as
//! `PWD=`, because the kernel has no process working directory.
//!
//! P4p adds shell pipelines and redirections: a pipeline stage runs in a child `rsh -c`, with its
//! standard descriptors carried through the P2n fd map; `<`, `>`, `>>`, `2>`, `2>&1` and
//! here-documents are applied by the shell that executes the command.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicU32, Ordering};

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_initfs::Initfs;
    use raven_proto_fs::{
        O_APPEND, O_CREAT, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY, S_IFDIR, S_IFMT, S_IFREG,
    };
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{RtSys, SpawnAttributes, SpawnOptions, spawn_with_attributes};
    use raven_rt::sync::Mutex;
    use raven_scheme::Client;
    use raven_shell_core::{Command, Connector, List, Pipeline, RedirKind, Table, Vars, parse};

    entry!(main);

    /// Longest accepted input line.
    const MAX_LINE: usize = 1024;
    /// Longest variable value kept before truncation.
    const MAX_VALUE: usize = 4096;
    /// Largest here-document body accepted (the pipe capacity is 64 KiB).
    const MAX_HEREDOC: usize = 16 * 1024;
    /// Commands searched for a bare program name.
    const PATH: [&str; 2] = ["/bin", "/sbin"];
    /// Prompt shown before every line.
    const PROMPT: &[u8] = b"raven# ";
    /// Read chunk for one console line.
    const CHUNK: usize = 256;
    /// Wait between console read attempts while the server reports no input ready.
    const CONSOLE_POLL_NS: u64 = 10_000_000;

    /// How reading a line ended.
    enum Line {
        /// A complete line without its terminator.
        Ok(String),
        /// `^C`: the line was discarded.
        Interrupted,
        /// `^D`: the console reached end of file.
        Eof,
        /// The console read failed.
        Failed(Errno),
    }

    /// One standard stream of a shell or command.
    #[derive(Clone, Copy)]
    enum Stream {
        /// The shell's `tty:console` handle.
        Console,
        /// An explicit file or pipe handle.
        Handle(Handle),
    }

    impl Stream {
        fn handle(self, tty: Handle) -> Result<Handle, Errno> {
            match self {
                Self::Console if tty == Handle::NONE => Err(Errno::EBADF),
                Self::Console => Ok(tty),
                Self::Handle(handle) => Ok(handle),
            }
        }

        fn is_console(self) -> bool {
            matches!(self, Self::Console)
        }

        /// Write all bytes, retrying partial writes.
        fn write_all(self, tty: Handle, bytes: &[u8]) -> Result<(), Errno> {
            let handle = self.handle(tty)?;
            let client = Client::new();
            let mut offset = 0usize;
            while offset < bytes.len() {
                let written = client.write(handle, 0, &bytes[offset..])?;
                if written == 0 {
                    return Err(Errno::EIO);
                }
                offset = offset.saturating_add(written);
            }
            Ok(())
        }

        /// Read up to `buffer.len()` bytes, waiting while the console has none ready.
        ///
        /// The console server is single-threaded and answers an idle read with `EAGAIN` so it
        /// stays free to dispatch queued writes (K536, K570); a console reader waits and retries
        /// instead of treating the idle read as a failed session. Other streams return their
        /// error unchanged.
        fn read(self, tty: Handle, buffer: &mut [u8]) -> Result<usize, Errno> {
            let handle = self.handle(tty)?;
            read_console_waiting(
                self.is_console(),
                Errno::EAGAIN,
                |buffer| Client::new().read(handle, 0, buffer),
                || {
                    let _ = raven_rt::nanosleep(CONSOLE_POLL_NS);
                },
                buffer,
            )
        }
    }

    /// The three standard streams of a command.
    #[derive(Clone, Copy)]
    struct Io {
        stdin: Stream,
        stdout: Stream,
        stderr: Stream,
    }

    /// The session thread's state: the shell, its exit status and a done word.
    ///
    /// `spawn` builds a 64 KiB stack image on the caller's stack, and this process is spawned
    /// with a 64 KiB stack, so the session runs on a thread with room for it.
    static SHELL: Mutex<Option<Shell>> = Mutex::new(None);
    static COMMAND: Mutex<Option<String>> = Mutex::new(None);
    static EXIT_CODE: Mutex<Option<i32>> = Mutex::new(None);
    static SHELL_DONE: AtomicU32 = AtomicU32::new(0);
    /// The parsed initfs table, shared with the session thread.
    static INITFS: Mutex<Option<Initfs<'static>>> = Mutex::new(None);
    /// Stack for the session thread.
    const SHELL_STACK: usize = 256 * 1024;

    struct Shell {
        vars: Table,
        exported: Vec<String>,
        status: i32,
        cwd: String,
        tty: Handle,
        io: Io,
        running: bool,
        pending: Vec<u8>,
    }

    impl Vars for Shell {
        fn get(&self, name: &str) -> Option<String> {
            self.vars.get(name)
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let command = if args.get(1) == Some("-c") {
            Some(args.get(2).unwrap_or("").to_string())
        } else {
            None
        };
        match run(args, command) {
            Ok(code) => code,
            Err(message) => {
                raven_rt::eprintln!("rsh: {}", message);
                1
            }
        }
    }

    fn run(args: raven_rt::Args, command: Option<String>) -> Result<i32, String> {
        let interactive = command.is_none();
        // The initfs is parsed on the first external command. A shell that only runs builtins
        // (and every `-c` pipeline stage) avoids the expensive whole-archive CRC validation.
        *INITFS.lock() = None;
        let io = io_from_args(&args);
        let needs_tty = interactive
            || io.stdin.is_console()
            || io.stdout.is_console()
            || io.stderr.is_console();
        let tty = if needs_tty {
            Client::new()
                .open("tty:console", flags(O_RDWR)?)
                .map_err(|error| format!("tty:console: {}", error.description()))?
        } else {
            Handle::NONE
        };

        let mut shell = Shell {
            vars: Table::new(),
            exported: Vec::new(),
            status: 0,
            cwd: "/".to_string(),
            tty,
            io,
            running: true,
            pending: Vec::new(),
        };
        shell.vars.set("PATH", "/bin:/sbin");
        shell.vars.set("HOME", "/");
        shell.vars.set("USER", "root");
        shell.vars.set("SHELL", "/bin/rsh");
        shell.vars.set("PWD", "/");
        for name in ["PATH", "HOME", "USER", "SHELL", "PWD"] {
            shell.exported.push(name.to_string());
        }
        if interactive {
            raven_rt::println!("rsh: ready");
        }
        *SHELL.lock() = Some(shell);
        *COMMAND.lock() = command;
        let thread = raven_rt::thread::spawn(shell_thread, 0, SHELL_STACK)
            .map_err(|error| format!("shell thread: {}", error.description()))?;
        // The thread owns the session; detaching it leaks its stack until exit, which is fine.
        drop(thread);
        while SHELL_DONE.load(Ordering::Acquire) == 0 {
            let _ = raven_rt::futex_wait(&SHELL_DONE, 0, None);
        }
        Ok(EXIT_CODE.lock().take().unwrap_or(1))
    }

    /// Read the fd handles supplied by a pipeline parent.
    fn io_from_args(args: &raven_rt::Args) -> Io {
        let stream = |name: &str| {
            args.env_get(name)
                .and_then(|value| value.parse::<u32>().ok())
                .map(|value| Stream::Handle(Handle(value)))
                .unwrap_or(Stream::Console)
        };
        Io {
            stdin: stream("RSH_FD0"),
            stdout: stream("RSH_FD1"),
            stderr: stream("RSH_FD2"),
        }
    }

    /// Entry of the session thread: run the shell loop and publish its status.
    fn shell_thread(_argument: usize) {
        let command = COMMAND.lock().take();
        let code = match SHELL.lock().take() {
            Some(mut shell) => match command {
                Some(text) => shell.run_text(&text),
                None => shell.run_loop(),
            },
            None => 1,
        };
        *EXIT_CODE.lock() = Some(code);
        SHELL_DONE.store(1, Ordering::Release);
        let _ = raven_rt::futex_wake(&SHELL_DONE, u32::MAX);
    }

    fn flags(value: u32) -> Result<u32, String> {
        fs::s11_flags(value).map_err(|error| format!("open flags: {}", error.description()))
    }

    impl Shell {
        /// Run the prompt/read/evaluate loop until `exit`, EOF or a console error.
        fn run_loop(&mut self) -> i32 {
            self.run_startup_script();
            while self.running {
                self.prompt();
                match self.read_command() {
                    Some(text) => {
                        if text.trim().is_empty() {
                            continue;
                        }
                        self.run_text(&text);
                    }
                    None => break,
                }
            }
            if self.tty != Handle::NONE {
                let _ = raven_rt::handle_close(self.tty);
            }
            self.status
        }

        /// Read one logical line, including here-document body lines.
        fn read_command(&mut self) -> Option<String> {
            let mut text = match self.read_line() {
                Line::Ok(text) => text,
                Line::Interrupted => {
                    // consd already echoed `^C`; start a fresh line.
                    self.status = 130;
                    return Some(String::new());
                }
                Line::Eof => return None,
                Line::Failed(error) => {
                    raven_rt::eprintln!("rsh: console: {}", error.description());
                    self.status = 1;
                    return None;
                }
            };
            loop {
                match parse(&text) {
                    Err(raven_shell_core::ParseError::IncompleteHereDoc) => {
                        match self.read_line() {
                            Line::Ok(more) => {
                                text.push('\n');
                                text.push_str(&more);
                            }
                            Line::Interrupted => {
                                self.status = 130;
                                return Some(String::new());
                            }
                            Line::Eof => {
                                raven_rt::eprintln!("rsh: here-document is not terminated");
                                self.status = 2;
                                return Some(String::new());
                            }
                            Line::Failed(error) => {
                                raven_rt::eprintln!("rsh: console: {}", error.description());
                                self.status = 1;
                                return None;
                            }
                        }
                    }
                    _ => return Some(text),
                }
            }
        }

        fn prompt(&self) {
            let _ = self.io.stdout.write_all(self.tty, PROMPT);
        }

        fn read_line(&mut self) -> Line {
            let mut line: Vec<u8> = Vec::new();
            let mut buffer = [0u8; CHUNK];
            loop {
                if let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
                    line.extend_from_slice(&self.pending[..index]);
                    self.pending.drain(..=index);
                    return Line::Ok(String::from_utf8_lossy(&line).to_string());
                }
                line.extend_from_slice(&self.pending);
                self.pending.clear();
                match self.io.stdin.read(self.tty, &mut buffer) {
                    Ok(0) => {
                        if line.is_empty() {
                            return Line::Eof;
                        }
                        return Line::Ok(String::from_utf8_lossy(&line).to_string());
                    }
                    Ok(count) => {
                        let bytes = &buffer[..count];
                        if let Some(index) = bytes.iter().position(|byte| *byte == b'\n') {
                            line.extend_from_slice(&bytes[..index]);
                            self.pending.extend_from_slice(&bytes[index + 1..]);
                            return Line::Ok(String::from_utf8_lossy(&line).to_string());
                        }
                        line.extend_from_slice(bytes);
                        if line.len() > MAX_LINE {
                            line.truncate(MAX_LINE);
                        }
                    }
                    Err(Errno::EINTR) => return Line::Interrupted,
                    Err(error) => return Line::Failed(error),
                }
            }
        }

        fn run_startup_script(&mut self) {
            let script = "/etc/rc.local";
            let mut file = match fs::open(&with_scheme(script), O_RDONLY) {
                Ok(file) => file,
                Err(_) => return,
            };
            let mut text = Vec::new();
            let mut buffer = [0u8; 512];
            loop {
                match file.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => text.extend_from_slice(&buffer[..count]),
                    Err(_) => break,
                }
            }
            let _ = file.close();
            for line in String::from_utf8_lossy(&text).lines() {
                self.run_text(line);
            }
        }

        /// Parse and run one complete command text (one interactive line or a `-c` script).
        fn run_text(&mut self, text: &str) -> i32 {
            match parse(text) {
                Ok(list) => self.run_list(&list),
                Err(error) => {
                    if error != raven_shell_core::ParseError::Empty {
                        raven_rt::eprintln!("rsh: {}", error.message());
                        self.status = 2;
                    }
                    self.status
                }
            }
        }

        fn run_list(&mut self, list: &List) -> i32 {
            for item in &list.items {
                let run = match item.connector {
                    Connector::Next => true,
                    Connector::And => self.status == 0,
                    Connector::Or => self.status != 0,
                };
                if !run {
                    continue;
                }
                self.status = self.run_pipeline(&item.pipeline);
                if !self.running {
                    break;
                }
            }
            self.status
        }

        fn run_pipeline(&mut self, pipeline: &Pipeline) -> i32 {
            if pipeline.commands.len() == 1 {
                return self.run_command(&pipeline.commands[0], self.io);
            }
            self.run_multi_pipeline(pipeline)
        }

        /// Run one command in this shell, with its redirections applied locally.
        fn run_command(&mut self, command: &Command, base: Io) -> i32 {
            let mut io = base;
            let mut opened = Vec::new();
            if self.apply_redirections(command, &mut io, &mut opened) != 0 {
                for handle in opened {
                    let _ = raven_rt::handle_close(handle);
                }
                return 2;
            }
            let words: Vec<String> = command
                .words
                .iter()
                .map(|word| raven_shell_core::expand(word, self, self.status))
                .collect();
            let status = if words.is_empty() || words[0].is_empty() {
                self.status
            } else if self.is_builtin(&words[0]) {
                self.run_builtin(&words[0], &words, &mut io)
            } else {
                self.run_external(&words, &io)
            };
            for handle in opened {
                let _ = raven_rt::handle_close(handle);
            }
            status
        }

        /// Run every stage of a pipeline in a child `rsh -c`, connected by P2j pipes.
        fn run_multi_pipeline(&mut self, pipeline: &Pipeline) -> i32 {
            let count = pipeline.commands.len();
            let mut pipes: Vec<(Handle, Handle)> = Vec::new();
            for _ in 0..count - 1 {
                match raven_rt::pipe::pipe() {
                    Ok(pair) => pipes.push(pair),
                    Err(error) => {
                        raven_rt::eprintln!("rsh: pipe: {}", error.description());
                        return 1;
                    }
                }
            }
            let mut children: Vec<Handle> = Vec::new();
            for (index, command) in pipeline.commands.iter().enumerate() {
                let mut io = self.io;
                if index > 0 {
                    io.stdin = Stream::Handle(pipes[index - 1].0);
                }
                if index + 1 < count {
                    io.stdout = Stream::Handle(pipes[index].1);
                }
                // The stage applies its own redirections in the child shell.
                let source = command.render();
                match self.spawn_program("/bin/rsh", &["/bin/rsh", "-c", &source], &io) {
                    Ok(child) => children.push(child),
                    Err(message) => {
                        raven_rt::eprintln!("rsh: {}", message);
                        for child in children {
                            let mut code = 0;
                            let _ = RtSys.proc_wait(child, &mut code);
                            let _ = raven_rt::handle_close(child);
                        }
                        for (read, write) in pipes {
                            let _ = raven_rt::handle_close(read);
                            let _ = raven_rt::handle_close(write);
                        }
                        return 126;
                    }
                }
                if index + 1 < count {
                    let _ = raven_rt::handle_close(pipes[index].1);
                }
                if index > 0 {
                    let _ = raven_rt::handle_close(pipes[index - 1].0);
                }
            }
            let mut status = 0;
            for child in children {
                let mut code = 0;
                if RtSys.proc_wait(child, &mut code).is_ok() {
                    status = code;
                }
                let _ = raven_rt::handle_close(child);
            }
            status
        }

        /// Apply redirections in source order to `io`; opened handles go into `opened`.
        fn apply_redirections(
            &mut self,
            command: &Command,
            io: &mut Io,
            opened: &mut Vec<Handle>,
        ) -> i32 {
            for redir in &command.redirs {
                if redir.kind == RedirKind::Dup {
                    let target = redir.dup.unwrap_or(1);
                    let Some(stream) = stream_at(io, target) else {
                        raven_rt::eprintln!(
                            "rsh: {}>&{}: unsupported descriptor",
                            redir.fd,
                            target
                        );
                        return 2;
                    };
                    if set_stream(io, redir.fd, stream).is_err() {
                        raven_rt::eprintln!(
                            "rsh: {}>&{}: unsupported descriptor",
                            redir.fd,
                            target
                        );
                        return 2;
                    }
                    continue;
                }
                let target = raven_shell_core::expand(&redir.target, self, self.status);
                match redir.kind {
                    RedirKind::In => match self.open_file(&target, O_RDONLY) {
                        Ok(handle) => {
                            if set_stream(io, redir.fd, Stream::Handle(handle)).is_err() {
                                let _ = raven_rt::handle_close(handle);
                                return 2;
                            }
                            opened.push(handle);
                        }
                        Err(error) => {
                            raven_rt::eprintln!("rsh: {}: {}", target, error.description());
                            return 2;
                        }
                    },
                    RedirKind::Out | RedirKind::Append => {
                        let flags = if redir.kind == RedirKind::Append {
                            O_WRONLY | O_CREAT | O_APPEND
                        } else {
                            O_WRONLY | O_CREAT | O_TRUNC
                        };
                        match self.open_file(&target, flags) {
                            Ok(handle) => {
                                if set_stream(io, redir.fd, Stream::Handle(handle)).is_err() {
                                    let _ = raven_rt::handle_close(handle);
                                    return 2;
                                }
                                opened.push(handle);
                            }
                            Err(error) => {
                                raven_rt::eprintln!("rsh: {}: {}", target, error.description());
                                return 2;
                            }
                        }
                    }
                    RedirKind::HereDoc => {
                        let body = redir.body.as_deref().unwrap_or("");
                        if body.len() > MAX_HEREDOC {
                            raven_rt::eprintln!("rsh: here-document is too large");
                            return 2;
                        }
                        let (read, write) = match raven_rt::pipe::pipe() {
                            Ok(pair) => pair,
                            Err(error) => {
                                raven_rt::eprintln!("rsh: pipe: {}", error.description());
                                return 2;
                            }
                        };
                        let mut offset = 0usize;
                        let bytes = body.as_bytes();
                        while offset < bytes.len() {
                            match Client::new().write(write, 0, &bytes[offset..]) {
                                Ok(0) => {
                                    let _ = raven_rt::handle_close(read);
                                    let _ = raven_rt::handle_close(write);
                                    raven_rt::eprintln!("rsh: here-document write failed");
                                    return 2;
                                }
                                Ok(written) => offset = offset.saturating_add(written),
                                Err(error) => {
                                    let _ = raven_rt::handle_close(read);
                                    let _ = raven_rt::handle_close(write);
                                    raven_rt::eprintln!(
                                        "rsh: here-document: {}",
                                        error.description()
                                    );
                                    return 2;
                                }
                            }
                        }
                        let _ = raven_rt::handle_close(write);
                        if set_stream(io, redir.fd, Stream::Handle(read)).is_err() {
                            let _ = raven_rt::handle_close(read);
                            return 2;
                        }
                        opened.push(read);
                    }
                    RedirKind::Dup => unreachable!(),
                }
            }
            0
        }

        fn open_file(&mut self, path: &str, flags: u32) -> Result<Handle, Errno> {
            let flags = fs::s11_flags(flags)?;
            Client::new().open(&with_scheme(&self.absolute(path)), flags)
        }

        fn is_builtin(&self, name: &str) -> bool {
            matches!(
                name,
                "cd" | "pwd"
                    | "echo"
                    | "exit"
                    | "set"
                    | "export"
                    | "unset"
                    | "env"
                    | "help"
                    | "type"
                    | "cat"
                    | "read"
                    | "true"
                    | "false"
                    | ":"
            )
        }

        fn run_builtin(&mut self, name: &str, words: &[String], io: &mut Io) -> i32 {
            match name {
                "cat" => self.builtin_cat(words, io),
                "read" => self.builtin_read(words, io),
                "cd" => self.builtin_cd(words),
                "pwd" => {
                    let text = format!("{}\n", self.cwd);
                    write_status(io.stdout.write_all(self.tty, text.as_bytes()))
                }
                "echo" => {
                    let mut text = words.get(1..).unwrap_or(&[]).join(" ");
                    text.push('\n');
                    write_status(io.stdout.write_all(self.tty, text.as_bytes()))
                }
                "exit" => {
                    let code = words
                        .get(1)
                        .and_then(|value| value.parse::<i32>().ok())
                        .unwrap_or(self.status);
                    self.running = false;
                    code
                }
                "true" | ":" => 0,
                "false" => 1,
                "set" => self.print_variables(io),
                "export" => self.builtin_export(words),
                "unset" => self.builtin_unset(words),
                "env" => {
                    let mut text = String::new();
                    for (key, value) in self.variables_in_order() {
                        if self.is_exported(&key) {
                            text.push_str(&format!("{}={}\n", key, value));
                        }
                    }
                    write_status(io.stdout.write_all(self.tty, text.as_bytes()))
                }
                "help" => {
                    let text = "builtins: cd pwd echo cat read exit set export unset env help type true false\n";
                    write_status(io.stdout.write_all(self.tty, text.as_bytes()))
                }
                "type" => self.builtin_type(words, io),
                other => {
                    raven_rt::eprintln!("rsh: {}: not a builtin", other);
                    1
                }
            }
        }

        fn builtin_cat(&mut self, words: &[String], io: &mut Io) -> i32 {
            if words.len() == 1 {
                return copy_stream(io.stdin, io, self.tty);
            }
            let mut status = 0;
            for path in &words[1..] {
                match self.open_file(path, O_RDONLY) {
                    Ok(handle) => {
                        if copy_stream(Stream::Handle(handle), io, self.tty) != 0 {
                            status = 1;
                        }
                        let _ = raven_rt::handle_close(handle);
                    }
                    Err(error) => {
                        let text = format!("rsh: cat: {}: {}\n", path, error.description());
                        let _ = io.stderr.write_all(self.tty, text.as_bytes());
                        status = 1;
                    }
                }
            }
            status
        }

        fn builtin_read(&mut self, words: &[String], io: &mut Io) -> i32 {
            let name = words.get(1).map(String::as_str).unwrap_or("REPLY");
            let mut line = Vec::new();
            let mut buffer = [0u8; 1];
            loop {
                match io.stdin.read(self.tty, &mut buffer) {
                    Ok(0) => break,
                    Ok(_) => {
                        if buffer[0] == b'\n' {
                            break;
                        }
                        if line.len() < MAX_VALUE {
                            line.push(buffer[0]);
                        }
                    }
                    Err(Errno::EINTR) => continue,
                    Err(error) => {
                        let text = format!("rsh: read: {}\n", error.description());
                        let _ = io.stderr.write_all(self.tty, text.as_bytes());
                        return 1;
                    }
                }
            }
            self.vars.set(name, String::from_utf8_lossy(&line).as_ref());
            if line.is_empty() { 1 } else { 0 }
        }

        fn builtin_cd(&mut self, words: &[String]) -> i32 {
            let target = match words.get(1) {
                Some(path) => self.absolute(path),
                None => self.vars.get("HOME").unwrap_or_else(|| "/".to_string()),
            };
            match fs::stat(&with_scheme(&target)) {
                Ok(stat) if stat.mode & S_IFMT == S_IFDIR => {
                    self.cwd = target;
                    self.vars.set("PWD", &self.cwd);
                    0
                }
                Ok(_) => {
                    raven_rt::eprintln!("rsh: cd: {}: not a directory", target);
                    1
                }
                Err(error) => {
                    raven_rt::eprintln!("rsh: cd: {}: {}", target, error.description());
                    1
                }
            }
        }

        fn builtin_export(&mut self, words: &[String]) -> i32 {
            if words.len() == 1 {
                for name in &self.exported {
                    if let Some(value) = self.vars.get(name) {
                        raven_rt::println!("{}={}", name, value);
                    }
                }
                return 0;
            }
            for word in &words[1..] {
                let (name, value) = match word.split_once('=') {
                    Some((name, value)) => (name, Some(value)),
                    None => (word.as_str(), None),
                };
                if name.is_empty() {
                    continue;
                }
                if let Some(value) = value {
                    self.vars.set(name, &truncate(value));
                }
                if !self.is_exported(name) {
                    self.exported.push(name.to_string());
                }
            }
            0
        }

        fn builtin_unset(&mut self, words: &[String]) -> i32 {
            for word in &words[1..] {
                self.vars.unset(word);
                self.exported.retain(|item| item != word);
            }
            0
        }

        fn builtin_type(&mut self, words: &[String], io: &mut Io) -> i32 {
            let Some(name) = words.get(1) else {
                return 0;
            };
            if self.is_builtin(name) {
                let text = format!("{} is a shell builtin\n", name);
                return write_status(io.stdout.write_all(self.tty, text.as_bytes()));
            }
            match self.resolve_program(name) {
                Some(path) => {
                    let text = format!("{} is {}\n", name, path);
                    write_status(io.stdout.write_all(self.tty, text.as_bytes()))
                }
                None => {
                    raven_rt::eprintln!("rsh: type: {}: not found", name);
                    1
                }
            }
        }

        fn run_external(&mut self, words: &[String], io: &Io) -> i32 {
            let name = &words[0];
            let Some(path) = self.resolve_program(name) else {
                raven_rt::eprintln!("rsh: {}: command not found", name);
                return 127;
            };
            let args: Vec<&str> = words.iter().map(String::as_str).collect();
            match self.spawn_program(&path, &args, io) {
                Ok(child) => wait_child(child, name),
                Err(message) => {
                    raven_rt::eprintln!("rsh: {}: {}", name, message);
                    126
                }
            }
        }

        /// Parse the initfs on first use; `-c` children that run only builtins skip this.
        fn initfs(&mut self) -> Option<Initfs<'static>> {
            if let Some(initfs) = *INITFS.lock() {
                return Some(initfs);
            }
            let image = raven_rt::map_initfs().ok()?;
            // The kernel validated the whole image before /init, so the shell only needs the
            // structure here; `Initfs::get` verifies each file's CRC when it is spawned.
            let initfs = Initfs::parse_unchecked(image).ok()?;
            *INITFS.lock() = Some(initfs);
            Some(initfs)
        }

        /// Spawn `path` with a child fd map for the non-console standard streams.
        fn spawn_program(&mut self, path: &str, args: &[&str], io: &Io) -> Result<Handle, String> {
            let initfs = match self.initfs() {
                Some(initfs) => initfs,
                None => return Err("initfs is not mapped".to_string()),
            };
            let mut env = self.environment();
            let mut fdmap: Vec<(i32, u32)> = Vec::new();
            let tty = self.tty;
            let mut child_io = *io;
            let mut captures: Vec<(Handle, Handle, Stream)> = Vec::new();
            for fd in [1u8, 2] {
                let stream = stream_at(io, fd).unwrap_or(Stream::Console);
                if !stream.is_console() {
                    continue;
                }
                let (read, write) = match raven_rt::pipe::pipe() {
                    Ok(pair) => pair,
                    Err(error) => {
                        for (read, write, _) in captures {
                            let _ = raven_rt::handle_close(read);
                            let _ = raven_rt::handle_close(write);
                        }
                        return Err(format!("pipe: {}", error.description()));
                    }
                };
                if set_stream(&mut child_io, fd, Stream::Handle(write)).is_err() {
                    let _ = raven_rt::handle_close(read);
                    let _ = raven_rt::handle_close(write);
                    for (read, write, _) in captures {
                        let _ = raven_rt::handle_close(read);
                        let _ = raven_rt::handle_close(write);
                    }
                    return Err("unsupported descriptor".to_string());
                }
                captures.push((read, write, stream));
            }
            for fd in [0u8, 1, 2] {
                let stream = stream_at(&child_io, fd).unwrap_or(Stream::Console);
                let handle = match stream {
                    Stream::Handle(handle) => handle,
                    Stream::Console => tty,
                };
                if handle == Handle::NONE {
                    continue;
                }
                // `spawn` installs self/initfs/namespace as handles 0..2, so the first
                // transferred descriptor is handle 3.
                let child = 3u32
                    .checked_add(
                        u32::try_from(fdmap.len())
                            .map_err(|_| "too many inherited descriptors".to_string())?,
                    )
                    .ok_or_else(|| "too many inherited descriptors".to_string())?;
                fdmap.push((i32::from(fd), handle.0));
                env.push(format!("RSH_FD{fd}={child}"));
            }
            let env_refs: Vec<&str> = env.iter().map(String::as_str).collect();
            let options = SpawnOptions {
                args,
                env: &env_refs,
            };
            let zero = [0u8; 128];
            let attributes = SpawnAttributes {
                fdmap: &fdmap,
                sigmask: &zero,
                sigdefault: &zero,
                ignored: &zero,
            };
            match spawn_with_attributes(&initfs, path, &options, &attributes) {
                Ok(child) => {
                    for (read, write, output) in captures {
                        let _ = raven_rt::handle_close(write);
                        let _ = copy_pipe(read, output, tty);
                        let _ = raven_rt::handle_close(read);
                    }
                    Ok(child)
                }
                Err(error) => {
                    for (read, write, _) in captures {
                        let _ = raven_rt::handle_close(read);
                        let _ = raven_rt::handle_close(write);
                    }
                    Err(format!("{error:?}"))
                }
            }
        }

        fn resolve_program(&mut self, name: &str) -> Option<String> {
            if name.contains('/') {
                let path = self.absolute(name);
                return self.is_regular_file(&path).then_some(path);
            }
            for directory in PATH {
                let path = format!("{directory}/{name}");
                if self.is_regular_file(&path) {
                    return Some(path);
                }
            }
            None
        }

        fn is_regular_file(&mut self, path: &str) -> bool {
            fs::stat(&with_scheme(path))
                .map(|stat| stat.mode & S_IFMT == S_IFREG)
                .unwrap_or(false)
        }

        /// The environment for a child: the exported variables plus this shell's `PWD`.
        fn environment(&self) -> Vec<String> {
            let mut environment = Vec::new();
            for (key, value) in self.variables_in_order() {
                if key == "PWD" {
                    continue;
                }
                if self.is_exported(&key) {
                    environment.push(format!("{}={}", key, value));
                }
            }
            environment.push(format!("PWD={}", self.cwd));
            environment
        }

        fn variables_in_order(&self) -> Vec<(String, String)> {
            self.vars.pairs.clone()
        }

        fn is_exported(&self, name: &str) -> bool {
            self.exported.iter().any(|item| item == name)
        }

        fn print_variables(&mut self, io: &mut Io) -> i32 {
            let mut pairs = self.variables_in_order();
            pairs.sort_by(|left, right| left.0.cmp(&right.0));
            let mut text = String::new();
            for (key, value) in pairs {
                text.push_str(&format!("{}={}\n", key, value));
            }
            write_status(io.stdout.write_all(self.tty, text.as_bytes()))
        }

        /// Resolve `path` against the shell's working directory.
        fn absolute(&self, path: &str) -> String {
            match path.find(':') {
                Some(index) if !path[..index].contains('/') => return path.to_string(),
                _ => {}
            }
            if path.starts_with('/') {
                return normalize(path);
            }
            if self.cwd == "/" {
                normalize(&format!("/{}", path))
            } else {
                normalize(&format!("{}/{}", self.cwd, path))
            }
        }
    }

    /// Wait for a child and return its exit status.
    fn wait_child(child: Handle, name: &str) -> i32 {
        let mut code = 0i32;
        let waited = RtSys.proc_wait(child, &mut code);
        let _ = raven_rt::handle_close(child);
        match waited {
            Ok(()) => code,
            Err(error) => {
                raven_rt::eprintln!("rsh: {}: {}", name, error.description());
                126
            }
        }
    }

    /// Copy a captured pipe to its console stream until EOF.
    fn copy_pipe(input: Handle, output: Stream, tty: Handle) -> i32 {
        let mut buffer = [0u8; 512];
        loop {
            match Client::new().read(input, 0, &mut buffer) {
                Ok(0) => return 0,
                Ok(count) => {
                    if output.write_all(tty, &buffer[..count]).is_err() {
                        return 1;
                    }
                }
                Err(Errno::EINTR) => continue,
                Err(_) => return 1,
            }
        }
    }

    /// Copy a stream to the command's stdout until EOF.
    fn copy_stream(input: Stream, io: &mut Io, tty: Handle) -> i32 {
        let mut buffer = [0u8; 512];
        loop {
            match input.read(tty, &mut buffer) {
                Ok(0) => return 0,
                Ok(count) => {
                    if let Err(error) = io.stdout.write_all(tty, &buffer[..count]) {
                        let text = format!("rsh: cat: {}\n", error.description());
                        let _ = io.stderr.write_all(tty, text.as_bytes());
                        return 1;
                    }
                }
                Err(Errno::EINTR) => continue,
                Err(error) => {
                    let text = format!("rsh: cat: {}\n", error.description());
                    let _ = io.stderr.write_all(tty, text.as_bytes());
                    return 1;
                }
            }
        }
    }

    fn write_status(result: Result<(), Errno>) -> i32 {
        match result {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("rsh: {}", error.description());
                1
            }
        }
    }

    fn stream_at(io: &Io, fd: u8) -> Option<Stream> {
        match fd {
            0 => Some(io.stdin),
            1 => Some(io.stdout),
            2 => Some(io.stderr),
            _ => None,
        }
    }

    fn set_stream(io: &mut Io, fd: u8, stream: Stream) -> Result<(), ()> {
        match fd {
            0 => io.stdin = stream,
            1 => io.stdout = stream,
            2 => io.stderr = stream,
            _ => return Err(()),
        }
        Ok(())
    }

    /// Lexically normalise an absolute path.
    fn normalize(path: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }
        let mut text = String::from("/");
        text.push_str(&parts.join("/"));
        text
    }

    fn with_scheme(path: &str) -> String {
        match path.find(':') {
            Some(index) if !path[..index].contains('/') => path.to_string(),
            _ => format!("file:{}", path),
        }
    }

    fn truncate(value: &str) -> String {
        if value.len() <= MAX_VALUE {
            value.to_string()
        } else {
            value[..MAX_VALUE].to_string()
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

/// Read through `read`, waiting while a console stream reports no input ready.
///
/// The console server is single-threaded and answers an idle read with `eagain` so it stays
/// free to dispatch queued writes (K536, K570). A console reader therefore waits and retries
/// instead of ending the session; other streams and errors are returned unchanged.
#[cfg(any(target_os = "none", test))]
fn read_console_waiting<E, R, W>(
    console: bool,
    eagain: E,
    mut read: R,
    mut wait: W,
    buffer: &mut [u8],
) -> Result<usize, E>
where
    E: Copy + PartialEq,
    R: FnMut(&mut [u8]) -> Result<usize, E>,
    W: FnMut(),
{
    loop {
        match read(buffer) {
            Err(error) if console && error == eagain => wait(),
            result => return result,
        }
    }
}

#[cfg(test)]
#[path = "main_ut.rs"]
mod tests;
