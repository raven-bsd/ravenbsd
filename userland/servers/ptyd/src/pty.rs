// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The pty queues and the foreground process group.

use alloc::collections::VecDeque;

use raven_abi::errno::Errno;
use raven_tty_core::{Discipline, Event, Output, Settings};

/// POSIX SIGINT.
pub const SIGINT: u32 = 2;

/// Where an interrupt/hangup signal is sent.
pub trait SignalSink {
    /// Signal a negative process group id.
    fn signal(&mut self, pgrp: i32, signo: u32) -> Result<(), Errno>;
}

/// One pty pair.
pub struct Pty<S: SignalSink> {
    discipline: Discipline,
    to_master: VecDeque<u8>,
    to_slave: VecDeque<u8>,
    eof: bool,
    fg_pgrp: i32,
    sink: S,
}

impl<S: SignalSink> Pty<S> {
    /// Create an empty canonical/echo pty pair.
    #[must_use]
    pub fn new(sink: S) -> Self {
        Self {
            discipline: Discipline::new(),
            to_master: VecDeque::new(),
            to_slave: VecDeque::new(),
            eof: false,
            fg_pgrp: 0,
            sink,
        }
    }

    /// Current mode/echo settings.
    #[must_use]
    pub fn settings(&self) -> Settings {
        self.discipline.settings()
    }

    /// Set the mode/echo settings and return the discarded pending input count.
    pub fn set_settings(&mut self, settings: Settings) -> usize {
        self.discipline.set_settings(settings)
    }

    /// Drive input from the master side through the discipline.
    pub fn write_master(&mut self, data: &[u8]) {
        let mut output = Output::default();
        self.discipline.feed(data, &mut output);
        self.to_master.extend(output.echo);
        self.to_slave.extend(output.data);
        for line in output.lines {
            self.to_slave.extend(line);
        }
        for event in output.events {
            match event {
                Event::Interrupt => {
                    if self.fg_pgrp > 0 {
                        let _ = self.sink.signal(-self.fg_pgrp, SIGINT);
                    }
                }
                Event::EndOfFile => self.eof = true,
            }
        }
    }

    /// Write output from the slave side; it becomes readable on the master.
    pub fn write_slave(&mut self, data: &[u8]) {
        self.to_master.extend(data.iter().copied());
    }

    /// Read bytes for the master side, or `EAGAIN` when no data is queued.
    pub fn read_master(&mut self, buffer: &mut [u8]) -> Result<usize, Errno> {
        take(&mut self.to_master, buffer, self.eof)
    }

    /// Read bytes for the slave side, or `EAGAIN` when no data is queued.
    pub fn read_slave(&mut self, buffer: &mut [u8]) -> Result<usize, Errno> {
        take(&mut self.to_slave, buffer, self.eof)
    }

    /// Discard all pending input and output bytes.
    pub fn flush(&mut self) -> u32 {
        let pending = self
            .discipline
            .flush()
            .saturating_add(self.to_master.len())
            .saturating_add(self.to_slave.len());
        self.to_master.clear();
        self.to_slave.clear();
        self.eof = false;
        u32::try_from(pending).unwrap_or(u32::MAX)
    }

    /// Set the foreground process group.
    pub fn set_fg(&mut self, pgrp: i32) {
        self.fg_pgrp = pgrp;
    }

    /// The foreground process group.
    #[must_use]
    pub const fn fg(&self) -> i32 {
        self.fg_pgrp
    }

    /// Hang up the pty; empty reads return zero from now on.
    pub fn hangup(&mut self) {
        self.eof = true;
    }

    /// True after a hangup/^D.
    #[must_use]
    pub const fn is_eof(&self) -> bool {
        self.eof
    }
}

fn take(queue: &mut VecDeque<u8>, buffer: &mut [u8], eof: bool) -> Result<usize, Errno> {
    if queue.is_empty() {
        return if eof { Ok(0) } else { Err(Errno::EAGAIN) };
    }
    let count = buffer.len().min(queue.len());
    let mut written = 0usize;
    while written < count {
        if let Some(byte) = queue.pop_front() {
            buffer[written] = byte;
            written = written.saturating_add(1);
        }
    }
    Ok(written)
}

#[cfg(test)]
#[path = "pty_ut.rs"]
mod tests;
