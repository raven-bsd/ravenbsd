// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `rsh`, the RavenBSD shell (K37).
//!
//! The shell owns the console through `tty:console` (K36), parses a line with `raven-shell-core`,
//! runs the builtins itself and spawns programs from the initfs (`raven_rt::spawn`), waiting for
//! each child and keeping `$?`. It keeps its own working directory and passes it to children as
//! `PWD=`, because the kernel has no process working directory.

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

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_initfs::Initfs;
    use raven_proto_fs::{
        O_APPEND, O_CREAT, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY, S_IFDIR, S_IFMT, S_IFREG,
    };
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{RtSys, SpawnError, SpawnOptions};
    use raven_rt::sync::Mutex;
    use raven_scheme::Client;
    use raven_shell_core::{Command, Connector, RedirKind, Table, Vars, parse};

    entry!(main);

    /// Longest accepted input line.
    const MAX_LINE: usize = 1024;
    /// Longest variable value kept before truncation.
    const MAX_VALUE: usize = 4096;
    /// Commands searched for a bare program name.
    const PATH: [&str; 2] = ["/bin", "/sbin"];
    /// Prompt shown before every line.
    const PROMPT: &[u8] = b"raven# ";
    /// Read chunk for one console line.
    const CHUNK: usize = 256;

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

    struct Shell {
        vars: Table,
        exported: Vec<String>,
        status: i32,
        cwd: String,
        tty: Handle,
        running: bool,
    }

    /// One external command handed to the spawn thread.
    struct Job {
        path: String,
        args: Vec<String>,
        env: Vec<String>,
    }

    /// How a spawned child ended.
    enum Child {
        Exit(i32),
        NotFound,
        Failed(String),
    }

    /// The spawn thread's input and output.
    ///
    /// `spawn` builds a 64 KiB stack image on the caller's stack, and this process is itself
    /// spawned with a 64 KiB stack, so the call runs on a thread with room for it.
    static JOB: Mutex<Option<Job>> = Mutex::new(None);
    static RESULT: Mutex<Option<Child>> = Mutex::new(None);
    /// The parsed initfs table, shared with the spawn thread.
    static INITFS: Mutex<Option<Initfs<'static>>> = Mutex::new(None);
    /// Stack for the spawn thread.
    const SPAWN_STACK: usize = 256 * 1024;

    impl Vars for Shell {
        fn get(&self, name: &str) -> Option<String> {
            self.vars.get(name)
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(code) => code,
            Err(message) => {
                raven_rt::eprintln!("rsh: {}", message);
                1
            }
        }
    }

    fn run() -> Result<i32, String> {
        let image =
            raven_rt::map_initfs().map_err(|error| format!("initfs: {}", error.description()))?;
        let initfs = Initfs::parse(image).map_err(|error| format!("initfs: {error}"))?;
        *INITFS.lock() = Some(initfs);
        let client = Client::new();
        let tty = client
            .open("tty:console", flags(O_RDWR)?)
            .map_err(|error| format!("tty:console: {}", error.description()))?;

        let mut shell = Shell {
            vars: Table::new(),
            exported: Vec::new(),
            status: 0,
            cwd: "/".to_string(),
            tty,
            running: true,
        };
        shell.vars.set("PATH", "/bin:/sbin");
        shell.vars.set("HOME", "/");
        shell.vars.set("USER", "root");
        shell.vars.set("SHELL", "/bin/rsh");
        shell.vars.set("PWD", "/");
        for name in ["PATH", "HOME", "USER", "SHELL", "PWD"] {
            shell.exported.push(name.to_string());
        }

        raven_rt::println!("rsh: ready");
        shell.run_startup_script();

        while shell.running {
            shell.prompt();
            match shell.read_line() {
                Line::Ok(text) => {
                    if text.trim().is_empty() {
                        continue;
                    }
                    shell.run_line(&text);
                }
                Line::Interrupted => {
                    // consd already echoed `^C`; start a fresh line.
                    shell.status = 130;
                }
                Line::Eof => break,
                Line::Failed(error) => {
                    raven_rt::eprintln!("rsh: console: {}", error.description());
                    return Ok(1);
                }
            }
        }
        let _ = client.close(tty);
        Ok(shell.status)
    }

    fn flags(value: u32) -> Result<u32, String> {
        fs::s11_flags(value).map_err(|error| format!("open flags: {}", error.description()))
    }

    impl Shell {
        fn prompt(&self) {
            let _ = Client::new().write(self.tty, 0, PROMPT);
        }

        fn read_line(&mut self) -> Line {
            let client = Client::new();
            let mut line: Vec<u8> = Vec::new();
            let mut buffer = [0u8; CHUNK];
            loop {
                match client.read(self.tty, 0, &mut buffer) {
                    Ok(0) => return Line::Eof,
                    Ok(count) => {
                        for &byte in &buffer[..count] {
                            if byte == b'\n' {
                                return Line::Ok(String::from_utf8_lossy(&line).to_string());
                            }
                            if line.len() < MAX_LINE {
                                line.push(byte);
                            }
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
                self.run_line(line);
            }
        }

        fn run_line(&mut self, line: &str) {
            let list = match parse(line) {
                Ok(list) => list,
                Err(error) => {
                    raven_rt::eprintln!("rsh: {}", error.message());
                    self.status = 2;
                    return;
                }
            };
            for item in list.items {
                let run = match item.connector {
                    Connector::Next => true,
                    Connector::And => self.status == 0,
                    Connector::Or => self.status != 0,
                };
                if !run {
                    continue;
                }
                self.status = self.run_command(&item.command);
                if !self.running {
                    return;
                }
            }
        }

        fn run_command(&mut self, command: &Command) -> i32 {
            let words: Vec<String> = command
                .words
                .iter()
                .map(|word| raven_shell_core::expand(word, self, self.status))
                .collect();
            if words.is_empty() || words[0].is_empty() {
                return self.status;
            }
            let name = words[0].clone();
            if self.is_builtin(&name) {
                let target = match self.redirection_target(command) {
                    Ok(target) => target,
                    Err(status) => return status,
                };
                return match target {
                    Some((kind, path)) => self.run_builtin_with_output(&name, &words, kind, &path),
                    None => self.run_builtin(&name, &words),
                };
            }
            if !command.redirs.is_empty() {
                raven_rt::eprintln!("rsh: redirection is only supported for builtins");
                return 2;
            }
            self.run_external(&words)
        }

        /// The output redirection of a command, if any; `<` is not supported.
        fn redirection_target(
            &mut self,
            command: &Command,
        ) -> Result<Option<(RedirKind, String)>, i32> {
            let mut target = None;
            for redir in &command.redirs {
                let path = raven_shell_core::expand(&redir.target, self, self.status);
                match redir.kind {
                    RedirKind::In => {
                        raven_rt::eprintln!("rsh: input redirection is not supported");
                        return Err(2);
                    }
                    kind => target = Some((kind, path)),
                }
            }
            Ok(target)
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
            )
        }

        fn run_builtin_with_output(
            &mut self,
            name: &str,
            words: &[String],
            kind: RedirKind,
            path: &str,
        ) -> i32 {
            let text = match name {
                "echo" => {
                    let mut text = words.get(1..).unwrap_or(&[]).join(" ");
                    text.push('\n');
                    text
                }
                "pwd" => format!("{}\n", self.cwd),
                _ => {
                    raven_rt::eprintln!(
                        "rsh: {}: redirection is only supported for echo and pwd",
                        name
                    );
                    return 2;
                }
            };
            let flags = match kind {
                RedirKind::Append => O_WRONLY | O_CREAT | O_APPEND,
                _ => O_WRONLY | O_CREAT | O_TRUNC,
            };
            match fs::open(&with_scheme(&self.absolute(path)), flags) {
                Ok(mut file) => match file.write(text.as_bytes()) {
                    Ok(_) => {
                        let _ = file.close();
                        0
                    }
                    Err(error) => {
                        let _ = file.close();
                        raven_rt::eprintln!("rsh: {}: {}", path, error.description());
                        1
                    }
                },
                Err(error) => {
                    raven_rt::eprintln!("rsh: {}: {}", path, error.description());
                    1
                }
            }
        }

        fn run_builtin(&mut self, name: &str, words: &[String]) -> i32 {
            match name {
                "cd" => self.builtin_cd(words),
                "pwd" => {
                    raven_rt::println!("{}", self.cwd);
                    0
                }
                "echo" => {
                    raven_rt::println!("{}", words.get(1..).unwrap_or(&[]).join(" "));
                    0
                }
                "exit" => {
                    let code = words
                        .get(1)
                        .and_then(|value| value.parse::<i32>().ok())
                        .unwrap_or(self.status);
                    self.running = false;
                    code
                }
                "set" => {
                    self.print_variables();
                    0
                }
                "export" => self.builtin_export(words),
                "unset" => self.builtin_unset(words),
                "env" => {
                    for (key, value) in self.variables_in_order() {
                        if self.is_exported(&key) {
                            raven_rt::println!("{}={}", key, value);
                        }
                    }
                    0
                }
                "help" => {
                    raven_rt::println!("builtins: cd pwd echo exit set export unset env help type");
                    0
                }
                "type" => self.builtin_type(words),
                other => {
                    raven_rt::eprintln!("rsh: {}: not a builtin", other);
                    1
                }
            }
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
            for name in &words[1..] {
                self.vars.unset(name);
                self.exported.retain(|item| item != name);
            }
            0
        }

        fn builtin_type(&mut self, words: &[String]) -> i32 {
            let Some(name) = words.get(1) else {
                return 0;
            };
            if self.is_builtin(name) {
                raven_rt::println!("{} is a shell builtin", name);
                return 0;
            }
            match self.resolve_program(name) {
                Some(path) => {
                    raven_rt::println!("{} is {}", name, path);
                    0
                }
                None => {
                    raven_rt::eprintln!("rsh: type: {}: not found", name);
                    1
                }
            }
        }

        fn run_external(&mut self, words: &[String]) -> i32 {
            let name = &words[0];
            let Some(path) = self.resolve_program(name) else {
                raven_rt::eprintln!("rsh: {}: command not found", name);
                return 127;
            };
            let job = Job {
                path,
                args: words.to_vec(),
                env: self.environment(),
            };
            *JOB.lock() = Some(job);
            let thread = match raven_rt::thread::spawn(spawn_thread, 0, SPAWN_STACK) {
                Ok(thread) => thread,
                Err(error) => {
                    raven_rt::eprintln!("rsh: {}: {}", name, error.description());
                    return 1;
                }
            };
            if let Err(error) = thread.join() {
                raven_rt::eprintln!("rsh: {}: {}", name, error.description());
                return 1;
            }
            match RESULT.lock().take() {
                Some(Child::Exit(code)) => code,
                Some(Child::NotFound) => {
                    raven_rt::eprintln!("rsh: {}: command not found", name);
                    127
                }
                Some(Child::Failed(message)) => {
                    raven_rt::eprintln!("rsh: {}: {}", name, message);
                    126
                }
                None => {
                    raven_rt::eprintln!("rsh: {}: spawned nothing", name);
                    126
                }
            }
        }

        /// Resolve a program name to its initfs path.
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

        fn print_variables(&self) {
            let mut pairs = self.variables_in_order();
            pairs.sort_by(|left, right| left.0.cmp(&right.0));
            for (key, value) in pairs {
                raven_rt::println!("{}={}", key, value);
            }
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

    /// Thread entry: run the queued child with a stack large enough for `spawn`.
    fn spawn_thread(_argument: usize) {
        let job = JOB.lock().take();
        let outcome = match job {
            Some(job) => run_child(&job),
            None => Child::Failed("spawn thread started without a job".to_string()),
        };
        *RESULT.lock() = Some(outcome);
    }

    /// Build the child's stack image, spawn it and wait for it.
    fn run_child(job: &Job) -> Child {
        let initfs = match *INITFS.lock() {
            Some(initfs) => initfs,
            None => return Child::Failed("initfs is not mapped".to_string()),
        };
        let arguments: Vec<&str> = job.args.iter().map(String::as_str).collect();
        let environment: Vec<&str> = job.env.iter().map(String::as_str).collect();
        let options = SpawnOptions {
            args: &arguments,
            env: &environment,
        };
        match raven_rt::spawn::spawn(&initfs, &job.path, &options) {
            Ok(process) => {
                let mut code = 0i32;
                let waited = RtSys.proc_wait(process, &mut code);
                let _ = raven_rt::handle_close(process);
                match waited {
                    Ok(()) => Child::Exit(code),
                    Err(error) => Child::Failed(error.description().to_string()),
                }
            }
            Err(SpawnError::NotFound) => Child::NotFound,
            Err(error) => Child::Failed(format!("{error:?}")),
        }
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
