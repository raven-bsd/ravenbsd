// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD-only startup timing trace for the interactive shell.
//!
//! `init` starts a monotonic clock at `main`; `mark` appends `stage<TAB>microseconds` to
//! `/tmp/brush-startup.tsv`. The file is read after the first prompt (the login-latency qtest
//! dumps it) so the normal shell output is unchanged. On non-RavenBSD targets every function is
//! a no-op, keeping host tests side-effect free.

#[cfg(target_os = "ravenbsd")]
mod imp {
    use std::io::Write as _;
    use std::sync::OnceLock;
    use std::time::Instant;

    static START: OnceLock<Instant> = OnceLock::new();

    /// Starts the monotonic clock and records the `main` stage.
    pub fn init() {
        START.get_or_init(Instant::now);
        mark("main");
    }

    /// Appends one stage and its microseconds since [`init`].
    pub fn mark(stage: &str) {
        let Some(start) = START.get() else {
            return;
        };
        let micros = start.elapsed().as_micros();
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/brush-startup.tsv")
        {
            let _ = writeln!(file, "{stage}\t{micros}");
        }
    }
}

#[cfg(not(target_os = "ravenbsd"))]
mod imp {
    /// No-op on non-RavenBSD targets (host tests stay side-effect free).
    pub fn init() {}
    /// No-op on non-RavenBSD targets.
    pub fn mark(_stage: &str) {}
}

pub use imp::{init, mark};
