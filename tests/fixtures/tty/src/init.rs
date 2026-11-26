// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P3 pty and terminal job-control fixture client.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_initfs::Initfs;
    use raven_libc_proc::consts::KERNEL_WCONTINUED;
    use raven_proto_fs::OpenFlags;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::println;
    use raven_rt::spawn::{SpawnOptions, spawn};
    use raven_scheme::Client;
    use raven_tty_fixture::{self_pgid, signal, tcgetpgrp, tcsetpgrp, tiocsctty};

    entry!(main);

    const VERSION: u16 = 1;
    const CALL_LEN: usize = 12;
    const OP_SET_MODE: u16 = 1;
    const OP_FLUSH: u16 = 3;
    const OP_PTY_SET_FG: u16 = 0x10;
    const OP_PTY_GET_FG: u16 = 0x11;
    const OP_PTY_HANGUP: u16 = 0x12;
    const OP_TTYD_SET_FG: u16 = 0x20;
    const OP_TTYD_GET_FG: u16 = 0x21;
    const OP_TTYD_HANGUP: u16 = 0x22;
    const OP_TTYD_STATE: u16 = 0x23;
    const OP_TTYD_SET_SID: u16 = 0x24;
    const OP_TTYD_GET_SID: u16 = 0x25;
    const OP_TTYD_SESSION_ID: u16 = 0x26;
    const SIGUSR1: u32 = 10;
    const SIGUSR2: u32 = 12;
    const SIGCONT: u32 = 18;
    const SIGTTIN: u32 = 21;
    const SIGTTOU: u32 = 22;
    const WUNTRACED: u32 = 2;
    // Raw `PROC_WAIT_EX` ABI from the libc constant source. The public `sys/wait.h` keeps the
    // musl/Linux value 8 and the libc backend translates it before this syscall.
    const WCONTINUED: u32 = KERNEL_WCONTINUED;

    const HUNG_BIT: u32 = 0x8000_0000;
    const PROC_HANDLE_INFO: usize = 0x3E;
    const PROC_SETPGID: usize = 0x36;
    const PROC_WAIT_EX: usize = 0x48;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("tty: ok");
                0
            }
            Err(step) => {
                raven_rt::println!("tty: fail {step}");
                1
            }
        }
    }

    fn run() -> Result<(), &'static str> {
        start_service("/sbin/initfsd", "initfs:")?;
        start_service("/sbin/ttyd", "ttyd:session")?;
        start_service("/sbin/ptyd", "pty:master")?;
        let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
        let initfs = Initfs::parse(image).map_err(|_| "initfs parse")?;
        let mode = fs::read_to_vec("initfs:/etc/tty-mode").map_err(|_| "mode")?;
        let mode = core::str::from_utf8(&mode).map_err(|_| "mode utf8")?.trim();
        match mode {
            "pty" => pty_cases(),
            "job" => job_cases(&initfs),
            "control" => control_cases(&initfs),
            _ => Err("mode"),
        }
    }

    fn start_service(path: &str, probe: &str) -> Result<(), &'static str> {
        let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
        let initfs = Initfs::parse(image).map_err(|_| "initfs parse")?;
        spawn(
            &initfs,
            path,
            &SpawnOptions {
                args: &[],
                env: &[],
            },
        )
        .map_err(|_| "spawn service")?;
        wait_scheme(probe)
    }

    fn wait_scheme(path: &str) -> Result<(), &'static str> {
        let client = Client::new();
        let mut attempt = 0usize;
        while attempt < 3000 {
            if let Ok(handle) = client.open(path, u32::from(OpenFlags::READ.to_wire())) {
                let _ = client.close(handle);
                return Ok(());
            }
            let _ = raven_rt::nanosleep(10_000_000);
            attempt = attempt.saturating_add(1);
        }
        Err("scheme timeout")
    }

    fn pty_cases() -> Result<(), &'static str> {
        let client = Client::new();
        let master = client
            .open("pty:master", u32::from(OpenFlags::READ_WRITE.to_wire()))
            .map_err(|_| "open master")?;
        let slave = client
            .open("pty:slave", u32::from(OpenFlags::READ_WRITE.to_wire()))
            .map_err(|_| "open slave")?;

        set_mode(&client, master, 0, 1)?;
        write_all(&client, master, b"hello\n")?;
        let slave_data = read_retry(&client, slave)?;
        if slave_data != b"hello\n" {
            return Err("canonical slave");
        }
        raven_rt::println!("tty: canonical=hello");
        let echo = read_retry(&client, master)?;
        if !echo.starts_with(b"hello") {
            return Err("canonical echo");
        }
        raven_rt::println!("tty: echo=hello");

        set_mode(&client, master, 1, 0)?;
        write_all(&client, master, b"raw")?;
        if read_retry(&client, slave)? != b"raw" {
            return Err("raw slave");
        }
        raven_rt::println!("tty: raw=raw");

        set_mode(&client, master, 0, 0)?;
        write_all(&client, master, b"quiet\n")?;
        if read_retry(&client, slave)? != b"quiet\n" {
            return Err("echo-off slave");
        }
        if !read_now(&client, master)?.is_empty() {
            return Err("echo-off leaked");
        }
        raven_rt::println!("tty: echo-off=ok");

        write_all(&client, master, b"junk")?;
        let (op, flushed, _) = tty_call(&client, master, OP_FLUSH, 0, 0)?;
        if op != OP_FLUSH || flushed < 4 {
            return Err("flush");
        }
        raven_rt::println!("tty: flush=ok");

        let _ = client.close(master);
        let _ = client.close(slave);
        raven_rt::println!("tty: pty-session=ok");
        Ok(())
    }

    fn job_cases(initfs: &Initfs<'_>) -> Result<(), &'static str> {
        let client = Client::new();
        let (pid, _child) = spawn_child(initfs)?;
        raven_rt::println!("tty: child={pid}");
        set_pgid(pid)?;

        let master = client
            .open("pty:master", u32::from(OpenFlags::READ_WRITE.to_wire()))
            .map_err(|_| "open master")?;
        let slave = client
            .open("pty:slave", u32::from(OpenFlags::READ_WRITE.to_wire()))
            .map_err(|_| "open slave")?;
        let session = client
            .open("ttyd:session", u32::from(OpenFlags::READ_WRITE.to_wire()))
            .map_err(|_| "open ttyd")?;
        let (_, _, _) = tty_call(&client, master, OP_PTY_SET_FG, pid as u32, 0)?;
        let (_, _, _) = tty_call(&client, session, OP_TTYD_SET_FG, pid as u32, 0)?;

        let _ = raven_rt::nanosleep(150_000_000);
        write_all(&client, master, b"\x03")?;
        let status = wait_child(pid)?;
        if exited(status) != Some(130) {
            return Err("job wait");
        }
        raven_rt::println!("tty: job wait=130");

        let (_, pty_fg, _) = tty_call(&client, master, OP_PTY_GET_FG, 0, 0)?;
        let (_, ttyd_fg, _) = tty_call(&client, session, OP_TTYD_GET_FG, 0, 0)?;
        if pty_fg != pid as u32 || ttyd_fg != pid as u32 {
            return Err("foreground");
        }
        raven_rt::println!("tty: pty-fg={pty_fg}");
        raven_rt::println!("tty: ttyd-fg={ttyd_fg}");

        let (_, _, _) = tty_call(&client, master, OP_PTY_HANGUP, 0, 0)?;
        if !read_retry(&client, slave)?.is_empty() {
            return Err("hangup data");
        }
        raven_rt::println!("tty: hangup=eof");
        let (_, _, _) = tty_call(&client, session, OP_TTYD_HANGUP, 0, 0)?;
        let (_, _, state) = tty_call(&client, session, OP_TTYD_STATE, 0, 0)?;
        if state & HUNG_BIT == 0 {
            return Err("ttyd hung");
        }
        raven_rt::println!("tty: ttyd-hung=ok");

        let _ = client.close(master);
        let _ = client.close(slave);
        let _ = client.close(session);
        raven_rt::println!("tty: tty-job-control=ok");
        Ok(())
    }

    fn set_mode(client: &Client, file: Handle, mode: u32, echo: u32) -> Result<(), &'static str> {
        let (op, arg0, arg1) = tty_call(client, file, OP_SET_MODE, mode, echo)?;
        if op != 2 || arg0 != mode || arg1 != echo {
            return Err("set mode");
        }
        Ok(())
    }

    fn tty_call(
        client: &Client,
        file: Handle,
        op: u16,
        arg0: u32,
        arg1: u32,
    ) -> Result<(u16, u32, u32), &'static str> {
        let mut request = [0u8; CALL_LEN];
        request[..2].copy_from_slice(&VERSION.to_le_bytes());
        request[2..4].copy_from_slice(&op.to_le_bytes());
        request[4..8].copy_from_slice(&arg0.to_le_bytes());
        request[8..12].copy_from_slice(&arg1.to_le_bytes());
        let mut response = [0u8; CALL_LEN];
        let length = client
            .call(file, &request, &mut response)
            .map_err(|_| "tty call")?;
        if length < CALL_LEN {
            return Err("tty call length");
        }
        let reply_op = u16::from_le_bytes([response[2], response[3]]);
        let reply0 = u32::from_le_bytes([response[4], response[5], response[6], response[7]]);
        let reply1 = u32::from_le_bytes([response[8], response[9], response[10], response[11]]);
        Ok((reply_op, reply0, reply1))
    }

    fn write_all(client: &Client, file: Handle, data: &[u8]) -> Result<(), &'static str> {
        let mut offset = 0usize;
        while offset < data.len() {
            let written = client
                .write(file, 0, data.get(offset..).ok_or("write range")?)
                .map_err(|_| "write")?;
            if written == 0 {
                return Err("short write");
            }
            offset = offset.saturating_add(written);
        }
        Ok(())
    }

    fn read_retry(client: &Client, file: Handle) -> Result<Vec<u8>, &'static str> {
        let mut buffer = [0u8; 128];
        let mut attempt = 0usize;
        while attempt < 200 {
            match client.read(file, 0, &mut buffer) {
                Ok(0) => return Ok(Vec::new()),
                Ok(count) => return Ok(buffer.get(..count).unwrap_or(&[]).to_vec()),
                Err(Errno::EAGAIN) => {
                    let _ = raven_rt::nanosleep(10_000_000);
                    attempt = attempt.saturating_add(1);
                }
                Err(_) => return Err("read"),
            }
        }
        Err("read timeout")
    }

    fn read_now(client: &Client, file: Handle) -> Result<Vec<u8>, &'static str> {
        let mut buffer = [0u8; 128];
        match client.read(file, 0, &mut buffer) {
            Ok(0) | Err(Errno::EAGAIN) => Ok(Vec::new()),
            Ok(count) => Ok(buffer.get(..count).unwrap_or(&[]).to_vec()),
            Err(_) => Err("read now"),
        }
    }

    fn control_cases(initfs: &Initfs<'_>) -> Result<(), &'static str> {
        let client = Client::new();
        let session = client
            .open("ttyd:session", u32::from(OpenFlags::READ_WRITE.to_wire()))
            .map_err(|_| "open ttyd")?;
        let (_, sid, _) = tty_call(&client, session, OP_TTYD_SESSION_ID, 0, 0)?;
        let init_pgid = self_pgid().map_err(|_| "pgid")?;
        let attach = tiocsctty(session).is_ok();
        let fg_ok = tcgetpgrp(session) == Ok(init_pgid);
        let ctty = attach && fg_ok;
        let _ = tty_call(&client, session, OP_TTYD_SET_SID, init_pgid as u32, 0)?;
        let (_, ttyd_sid, _) = tty_call(&client, session, OP_TTYD_GET_SID, 0, 0)?;
        let _ = tty_call(&client, session, OP_TTYD_SET_FG, init_pgid as u32, 0)?;
        let (_, ttyd_fg, _) = tty_call(&client, session, OP_TTYD_GET_FG, 0, 0)?;
        let ttyd_ok = ttyd_sid == init_pgid as u32 && ttyd_fg == init_pgid as u32;

        let sid_text = format!("{sid}");
        let (pid, _child) = spawn_program(
            initfs,
            "/bin/tty-ctl-child",
            &["tty-ctl-child", sid_text.as_str()],
        )?;
        set_pgid(pid)?;
        sleep_ms(200);

        tcsetpgrp(session, pid).map_err(|_| "orphan fg")?;
        let orphan = client.write(session, 0, b"o") == Err(Errno::EIO);

        tcsetpgrp(session, init_pgid).map_err(|_| "ttin fg")?;
        signal(pid, SIGUSR1).map_err(|_| "usr1")?;
        let stop = wait_child_opts(pid, WUNTRACED)?;
        let ttin = stopped_signal(stop) == Some(SIGTTIN);
        signal(pid, SIGCONT).map_err(|_| "cont1")?;
        let _ = wait_child_opts(pid, WCONTINUED)?;

        tcsetpgrp(session, pid).map_err(|_| "fg read")?;
        signal(pid, SIGUSR1).map_err(|_| "usr1b")?;
        sleep_ms(150);

        tcsetpgrp(session, init_pgid).map_err(|_| "ttou fg")?;
        signal(pid, SIGUSR2).map_err(|_| "usr2")?;
        let stop = wait_child_opts(pid, WUNTRACED)?;
        let ttou = stopped_signal(stop) == Some(SIGTTOU);
        signal(pid, SIGCONT).map_err(|_| "cont2")?;
        let _ = wait_child_opts(pid, WCONTINUED)?;

        tcsetpgrp(session, pid).map_err(|_| "fg write")?;
        signal(pid, SIGUSR2).map_err(|_| "usr2b")?;
        let done = wait_child_opts(pid, 0)?;
        let sig = ttin && ttou && exited(done) == Some(0);

        let (hup, _hup_child) = spawn_program(initfs, "/bin/tty-hup-child", &["tty-hup-child"])?;
        let _ = wait_child_opts(hup, 0)?;
        sleep_ms(500);

        println!(
            "tty-control: ctty={} fg={}",
            u8::from(attach),
            u8::from(fg_ok)
        );
        println!(
            "tty-control: ttyd={} orphan={}",
            u8::from(ttyd_ok),
            u8::from(orphan)
        );
        println!("tty-control: sig={}", u8::from(sig));
        println!("tty-control: hangup=1");
        let _ = client.close(session);
        if !(ctty && ttyd_ok && orphan && sig) {
            return Err("control checks");
        }
        println!("tty-control: ok");
        Ok(())
    }

    fn stopped_signal(status: i32) -> Option<u32> {
        if status & 0x7f == 0x7f {
            Some(((status >> 8) & 0xff) as u32)
        } else {
            None
        }
    }

    fn sleep_ms(milliseconds: u64) {
        let _ = raven_rt::nanosleep(milliseconds.saturating_mul(1_000_000));
    }

    fn spawn_program(
        initfs: &Initfs<'_>,
        path: &str,
        argv: &[&str],
    ) -> Result<(i32, Handle), &'static str> {
        let options = SpawnOptions {
            args: argv,
            env: &[],
        };
        let child = spawn(initfs, path, &options).map_err(|_| "spawn child")?;
        let pid = child_pid(child)?;
        Ok((pid, child))
    }

    fn spawn_child(initfs: &Initfs<'_>) -> Result<(i32, Handle), &'static str> {
        spawn_program(initfs, "/bin/tty-child", &["tty-child", "sleep"])
    }

    fn child_pid(child: Handle) -> Result<i32, &'static str> {
        // SAFETY: PROC_HANDLE_INFO reads the pid from a process handle owned by the caller.
        let pid = unsafe { raven_rt::sys::syscall1(PROC_HANDLE_INFO, child.0 as usize) };
        if pid < 0 {
            return Err("handle info");
        }
        i32::try_from(pid).map_err(|_| "pid range")
    }

    fn set_pgid(pid: i32) -> Result<(), &'static str> {
        // SAFETY: PROC_SETPGID takes the pid and the new group (0 = own group).
        let result = unsafe { raven_rt::sys::syscall2(PROC_SETPGID, pid as usize, 0) };
        if result < 0 {
            return Err("setpgid");
        }
        Ok(())
    }

    fn wait_child(pid: i32) -> Result<i32, &'static str> {
        wait_child_opts(pid, 0)
    }

    fn wait_child_opts(pid: i32, options: u32) -> Result<i32, &'static str> {
        let mut status = 0i32;
        // SAFETY: PROC_WAIT_EX writes one i32 status to a stack slot owned by this frame.
        let result = unsafe {
            raven_rt::sys::syscall4(
                PROC_WAIT_EX,
                1,
                pid as usize,
                options as usize,
                &mut status as *mut i32 as usize,
            )
        };
        if result < 0 {
            return Err("wait");
        }
        Ok(status)
    }

    fn exited(status: i32) -> Option<i32> {
        if status & 0x7f == 0 {
            Some((status >> 8) & 0xff)
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
