// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K548 rev348: a minimal Rust std `Command` canary for the brush child-fault.
//
// The brush builds its external command in `compose_std_command` (arg0, args, current_dir,
// env_clear+envs, three stdio, inject_fds) and the std `Command` reaches the libc
// `posix_spawn`. Every raw libc-spawn shape is already clean under a C canary; this program
// reproduces the same std `Command` sequence on the guest and checks a 256 KiB parent heap
// after the child. If the heap is intact, the std `Command` is not the corruptor and the
// remaining trigger is the brush shell's own heap/OpenFiles state.

use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

fn main() {
    let canary = vec![0xA5u8; 256 * 1024];
    let status = Command::new("/bin/libc-test")
        .arg0("libc-test")
        .arg("child")
        .env_clear()
        .envs(std::env::vars())
        .current_dir("/")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
    let canary_ok = canary.iter().all(|&byte| byte == 0xA5);
    let status_ok = status.is_ok_and(|value| value.success());
    println!("EXEC-CANARY status_ok={status_ok} canary_ok={canary_ok}");
}
