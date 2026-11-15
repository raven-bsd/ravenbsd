// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `man`, the RavenBSD manual page viewer.
//!
//! The target program reads pages from the initfs, renders them with `raven-mdoc` and uses the
//! compiled Raven Violet palette. Piped output is plain because the shell has no TTY here; an
//! explicit `--color=always` is the only way to force escapes.

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

    use man::{Action, Invocation};
    use raven_proto_fs::{O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{RtSys, SpawnOptions, spawn_image};
    use raven_rt::sync::Mutex;
    use raven_rt::thread;

    entry!(main);

    const MAX_PAGE: usize = 4 * 1024 * 1024;
    const PAGE_FILE: &str = "/tmp/man.page";
    const PAGER_STACK: usize = 256 * 1024;

    struct PagerJob {
        program: String,
        extra: Vec<String>,
        image: Vec<u8>,
    }

    static PAGER_JOB: Mutex<Option<PagerJob>> = Mutex::new(None);
    static PAGER_CODE: Mutex<Option<i32>> = Mutex::new(None);
    static PAGER_DONE: AtomicU32 = AtomicU32::new(0);

    fn main(args: raven_rt::Args) -> i32 {
        let arguments: Vec<&str> = args.iter().skip(1).collect();
        let invocation = match man::parse(&arguments) {
            Ok(invocation) => invocation,
            Err(error) => {
                raven_rt::eprintln!("man: {error}");
                return 2;
            }
        };
        match invocation.action {
            Action::Help => {
                raven_rt::println!("{}", man::USAGE);
                0
            }
            Action::Version => {
                raven_rt::println!("{}", man::version_line());
                0
            }
            Action::Render => render(&args, &invocation),
        }
    }

    fn render(args: &raven_rt::Args, invocation: &Invocation) -> i32 {
        let name = invocation.name.as_deref().unwrap_or("");
        let config = read_text("/etc/man.conf").unwrap_or_default();
        let roots = man::roots(&config, args.env_get("MANPATH"));
        let candidates = man::candidates(&roots, name, invocation.section);
        let mut found = None;
        for (section, path) in candidates {
            match read_page(&path) {
                Ok(data) => {
                    found = Some((section, path, data));
                    break;
                }
                Err(_) => continue,
            }
        }
        let Some((_section, path, data)) = found else {
            match invocation.section {
                Some(section) => {
                    raven_rt::eprintln!("man: no entry for {name} in section {section}");
                }
                None => raven_rt::eprintln!("man: no entry for {name}"),
            }
            return 1;
        };
        let source = match core::str::from_utf8(&data) {
            Ok(source) => source,
            Err(_) => {
                raven_rt::eprintln!("man: {path}: not valid UTF-8");
                return 1;
            }
        };
        let width = man::width(args.env_get("MANWIDTH"), args.env_get("COLUMNS"));
        let palette = man::palette_from_env_spec(args.env_get("RAVEN_PALETTE"));
        let term = args.env_get("TERM");
        let term_dumb = term == Some("dumb");
        let is_tty = term.is_some_and(|value| !value.is_empty());
        let enabled = man::color_enabled(
            invocation.color,
            is_tty,
            args.env_get("NO_COLOR").is_some(),
            term_dumb,
        );
        let output = match man::render_page(
            source,
            width,
            &palette,
            enabled,
            args.env_get("COLORTERM"),
            term,
        ) {
            Ok(output) => output,
            Err(error) => {
                raven_rt::eprintln!("man: {path}: {error}");
                return 1;
            }
        };

        let pager = args
            .env_get("MANPAGER")
            .filter(|value| !value.trim().is_empty())
            .or_else(|| args.env_get("PAGER"))
            .filter(|value| !value.trim().is_empty());
        if let Some(pager) = pager
            && let Some(argv) = man::pager(Some(pager))
        {
            match man::resolve_pager(&argv, |path| fs::stat(&scheme(path)).is_ok()) {
                Some(program) => match page_through(&program, &argv[1..], &output) {
                    Ok(()) => return 0,
                    Err(error) => raven_rt::eprintln!("man: {error}"),
                },
                None => {
                    raven_rt::eprintln!(
                        "man: pager `{}` not found; writing directly",
                        argv.first().map(String::as_str).unwrap_or("")
                    );
                }
            }
        }
        raven_rt::print!("{output}");
        0
    }

    fn page_through(program: &str, extra: &[String], output: &str) -> Result<(), String> {
        write_file(PAGE_FILE, output)?;
        let image = match read_bytes(program) {
            Ok(image) => image,
            Err(error) => {
                let _ = fs::unlink(&scheme(PAGE_FILE));
                return Err(error);
            }
        };
        *PAGER_JOB.lock() = Some(PagerJob {
            program: program.to_string(),
            extra: extra.to_vec(),
            image,
        });
        PAGER_DONE.store(0, Ordering::Release);
        let thread = match thread::spawn(pager_thread, 0, PAGER_STACK) {
            Ok(thread) => thread,
            Err(error) => {
                let _ = fs::unlink(&scheme(PAGE_FILE));
                return Err(error.description().to_string());
            }
        };
        while PAGER_DONE.load(Ordering::Acquire) == 0 {
            let _ = raven_rt::futex_wait(&PAGER_DONE, 0, None);
        }
        let code = PAGER_CODE.lock().take().unwrap_or(1);
        drop(thread);
        let _ = fs::unlink(&scheme(PAGE_FILE));
        if code == 0 {
            Ok(())
        } else {
            Err(format!("pager exited {code}"))
        }
    }

    fn pager_thread(_argument: usize) {
        let code = match PAGER_JOB.lock().take() {
            Some(job) => run_pager(&job),
            None => 1,
        };
        *PAGER_CODE.lock() = Some(code);
        PAGER_DONE.store(1, Ordering::Release);
        let _ = raven_rt::futex_wake(&PAGER_DONE, u32::MAX);
    }

    fn run_pager(job: &PagerJob) -> i32 {
        let mut arguments: Vec<&str> = Vec::with_capacity(job.extra.len() + 2);
        arguments.push(job.program.as_str());
        arguments.extend(job.extra.iter().map(String::as_str));
        arguments.push(PAGE_FILE);
        let process = match spawn_image(
            &job.image,
            &job.program,
            &SpawnOptions {
                args: &arguments,
                env: &[],
            },
        ) {
            Ok(process) => process,
            Err(error) => {
                raven_rt::eprintln!("man: {}: {error:?}", job.program);
                return 1;
            }
        };
        let mut code = 0i32;
        let waited = RtSys.proc_wait(process, &mut code);
        let _ = raven_rt::handle_close(process);
        if let Err(error) = waited {
            raven_rt::eprintln!("man: wait: {}", error.description());
            return 1;
        }
        code
    }

    fn read_page(path: &str) -> Result<Vec<u8>, String> {
        read_bytes(path).map(|data| {
            if data.len() > MAX_PAGE {
                data.get(..MAX_PAGE).unwrap_or(&data).to_vec()
            } else {
                data
            }
        })
    }

    fn read_text(path: &str) -> Result<String, String> {
        let data = read_bytes(path)?;
        String::from_utf8(data).map_err(|_| format!("{path}: not valid UTF-8"))
    }

    fn read_bytes(path: &str) -> Result<Vec<u8>, String> {
        let mut file = fs::open(&scheme(path), O_RDONLY)
            .map_err(|error| format!("{path}: {}", error.description()))?;
        let data = file
            .read_to_vec()
            .map_err(|error| error.description().to_string());
        let _ = file.close();
        data
    }

    fn write_file(path: &str, text: &str) -> Result<(), String> {
        let mut file = fs::open(&scheme(path), O_WRONLY | O_CREAT | O_TRUNC)
            .map_err(|error| format!("{path}: {}", error.description()))?;
        let result = write_all(&mut file, text.as_bytes());
        let _ = file.close();
        result.map_err(|error| format!("{path}: {error}"))
    }

    fn write_all(file: &mut fs::File, data: &[u8]) -> Result<(), String> {
        let mut offset = 0usize;
        while offset < data.len() {
            let written = file
                .write(data.get(offset..).unwrap_or(&[]))
                .map_err(|error| error.description().to_string())?;
            if written == 0 {
                return Err(String::from("short write"));
            }
            offset = offset.saturating_add(written);
        }
        Ok(())
    }

    fn scheme(path: &str) -> String {
        if fs::has_scheme(path) {
            path.to_string()
        } else {
            format!("file:{path}")
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
