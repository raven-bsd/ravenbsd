// SPDX-License-Identifier: MIT
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! K548 rev349: the brush's std `Command` sequence inside the same current-thread Tokio runtime
//! the shell uses on RavenBSD (entry.rs). The plain-main canary in rev348 was clean; the remaining
//! difference between it and the S4 brush is this runtime context (and the shell heap/OpenFiles).
//! The child is /bin/libc-test, the parent heap is a 256 KiB canary checked after status().

use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current-thread runtime");
    runtime.block_on(async {
        let canary = vec![0xA5u8; 256 * 1024];
        let status = Command::new("/bin/libc-test")
            .arg0("libc-test")
            .arg("child")
            .env_clear()
            .envs(std::env::vars())
            .current_dir("/home/raven")
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();
        let canary_ok = canary.iter().all(|&byte| byte == 0xA5);
        let status_ok = status.is_ok_and(|value| value.success());
        println!("EXEC-CANARY-TOKIO status_ok={status_ok} canary_ok={canary_ok}");
    });
}
