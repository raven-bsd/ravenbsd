// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable console multiplexer for `consd`.
//!
//! `tty:console` keeps the same line discipline and scheme contract; this module only decides
//! which input byte sources feed the discipline and which output sinks receive the echo and the
//! server's writes. UART is the built-in fallback, a keyboard source can be added, and output is
//! fanned out to every live sink (UART and, when present, a framebuffer text sink).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

use raven_tty_core::{Discipline, Event, Output, Settings};

/// Largest amount of backpressured output retained per sink.
pub const MAX_PENDING: usize = 64 * 1024;

/// A byte input source.
pub trait ConsoleInput: Send {
    /// Read available bytes. `WouldBlock` means no byte is ready; `EndOfFile`/`Closed` retire the
    /// source from the multiplexer.
    fn read(&mut self, out: &mut [u8]) -> Result<usize, InputError>;
}

/// A byte output sink.
pub trait ConsoleOutput: Send {
    /// Write bytes; a partial count or [`OutputError::Full`] is backpressure.
    fn write(&mut self, bytes: &[u8]) -> Result<usize, OutputError>;
}

/// Console input failure classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    /// No bytes are ready now.
    WouldBlock,
    /// The device/source reached end of input.
    EndOfFile,
    /// The device/source is gone.
    Closed,
    /// A transport error; the source is retired.
    Io,
}

/// Console output failure classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputError {
    /// The sink's bounded queue is full; retry later.
    Full,
    /// The sink is gone and is retired.
    Closed,
    /// A transport error; the sink is retired.
    Io,
}

struct OutputChannel {
    sink: Box<dyn ConsoleOutput>,
    pending: VecDeque<u8>,
    closed: bool,
}

impl OutputChannel {
    fn new(sink: Box<dyn ConsoleOutput>) -> Self {
        Self {
            sink,
            pending: VecDeque::new(),
            closed: false,
        }
    }
}

/// The multiplexed console state behind `tty:console`.
pub struct Multiplexer {
    discipline: Discipline,
    ready: VecDeque<u8>,
    interrupt: bool,
    eof: bool,
    write_allowed: bool,
    inputs: Vec<Box<dyn ConsoleInput>>,
    outputs: Vec<OutputChannel>,
}

impl Default for Multiplexer {
    fn default() -> Self {
        Self::new()
    }
}

impl Multiplexer {
    /// An empty multiplexer with the default canonical/echo discipline.
    pub const fn new() -> Self {
        Self {
            discipline: Discipline::new(),
            ready: VecDeque::new(),
            interrupt: false,
            eof: false,
            write_allowed: true,
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    /// Add an input source and return its index.
    pub fn add_input(&mut self, input: Box<dyn ConsoleInput>) -> usize {
        let index = self.inputs.len();
        self.inputs.push(input);
        index
    }

    /// Add an output sink and return its index.
    pub fn add_output(&mut self, output: Box<dyn ConsoleOutput>) -> usize {
        let index = self.outputs.len();
        self.outputs.push(OutputChannel::new(output));
        index
    }

    /// Number of live input sources.
    pub fn input_count(&self) -> usize {
        self.inputs.len()
    }

    /// Number of live output sinks.
    pub fn output_count(&self) -> usize {
        self.outputs
            .iter()
            .filter(|channel| !channel.closed)
            .count()
    }

    /// Current line settings.
    pub fn settings(&self) -> Settings {
        self.discipline.settings()
    }

    /// Apply new settings, dropping a partially edited line on a mode change.
    pub fn set_settings(&mut self, settings: Settings) -> usize {
        let discarded = self.discipline.set_settings(settings);
        if discarded > 0 {
            self.ready.clear();
            self.eof = false;
        }
        discarded
    }

    /// Drop buffered input, returning the number of discarded bytes.
    pub fn flush(&mut self) -> u32 {
        let pending = self.discipline.flush() + self.ready.len();
        self.ready.clear();
        self.eof = false;
        u32::try_from(pending).unwrap_or(u32::MAX)
    }

    /// Whether a reader can write to the console.
    pub fn write_allowed(&self) -> bool {
        self.write_allowed
    }

    /// Set the reader write permission.
    pub fn set_write_allowed(&mut self, allowed: bool) {
        self.write_allowed = allowed;
    }

    /// Consume one pending interrupt, if any.
    pub fn take_interrupt(&mut self) -> bool {
        let interrupt = self.interrupt;
        self.interrupt = false;
        interrupt
    }

    /// Whether all input sources have ended.
    pub fn eof(&self) -> bool {
        self.eof
    }

    /// Whether a scheme `READ` has bytes ready.
    pub fn has_ready(&self) -> bool {
        !self.ready.is_empty()
    }

    /// Mark the input side as ended (the reader thread lost its last source).
    pub fn mark_eof(&mut self) {
        self.eof = true;
    }

    /// Move ready bytes into `buffer`.
    pub fn take(&mut self, buffer: &mut [u8]) -> usize {
        let count = buffer.len().min(self.ready.len());
        for slot in buffer.iter_mut().take(count) {
            if let Some(byte) = self.ready.pop_front() {
                *slot = byte;
            }
        }
        count
    }

    /// Feed bytes from a backend reader thread and fan the echo out.
    ///
    /// Returns true when the read side became ready or an event was latched.
    pub fn feed(&mut self, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return false;
        }
        let mut output = Output::default();
        self.discipline.feed(bytes, &mut output);
        for line in &output.lines {
            self.ready.extend(line.iter().copied());
        }
        self.ready.extend(output.data.iter().copied());
        for event in &output.events {
            match event {
                Event::Interrupt => self.interrupt = true,
                Event::EndOfFile => self.eof = true,
            }
        }
        // Echo and fan-out are best effort: a disappeared sink must not stop the discipline.
        let _ = self.write_all(&output.echo);
        !output.lines.is_empty() || !output.data.is_empty() || !output.events.is_empty()
    }

    /// Poll every input once. Returns the number of bytes fed.
    pub fn pump(&mut self) -> usize {
        let mut fed = 0usize;
        let mut index = 0usize;
        while index < self.inputs.len() {
            let mut buffer = [0u8; 256];
            match self.inputs[index].read(&mut buffer) {
                Ok(0)
                | Err(InputError::EndOfFile)
                | Err(InputError::Closed)
                | Err(InputError::Io) => {
                    self.inputs.remove(index);
                    if self.inputs.is_empty() {
                        self.eof = true;
                    }
                }
                Ok(count) => {
                    let count = count.min(buffer.len());
                    self.feed(&buffer[..count]);
                    fed = fed.saturating_add(count);
                    index += 1;
                }
                Err(InputError::WouldBlock) => {
                    index += 1;
                }
            }
        }
        fed
    }

    /// Write bytes to every live sink, retaining backpressured bytes per sink.
    ///
    /// Returns the number of bytes accepted into the output queues; `Closed` when no sink is
    /// live, `Full` when a sink's bounded queue would overflow.
    pub fn write_all(&mut self, bytes: &[u8]) -> Result<usize, OutputError> {
        let _ = self.flush_outputs();
        if !self.outputs.iter().any(|channel| !channel.closed) {
            return Err(OutputError::Closed);
        }
        if bytes.len() > MAX_PENDING
            || self
                .outputs
                .iter()
                .any(|channel| !channel.closed && channel.pending.len() + bytes.len() > MAX_PENDING)
        {
            return Err(OutputError::Full);
        }
        for channel in &mut self.outputs {
            if !channel.closed {
                channel.pending.extend(bytes.iter().copied());
            }
        }
        let _ = self.flush_outputs();
        Ok(bytes.len())
    }

    /// Retry every sink's pending bytes. Full sinks stay queued; closed sinks are removed.
    pub fn flush_outputs(&mut self) -> Result<(), OutputError> {
        let mut first_error = None;
        for channel in &mut self.outputs {
            while !channel.closed && !channel.pending.is_empty() {
                channel.pending.make_contiguous();
                let (slice, _) = channel.pending.as_slices();
                match channel.sink.write(slice) {
                    Ok(count) if count > 0 => {
                        channel.pending.drain(..count.min(channel.pending.len()));
                    }
                    Ok(_) | Err(OutputError::Full) => {
                        if first_error.is_none() {
                            first_error = Some(OutputError::Full);
                        }
                        break;
                    }
                    Err(error) => {
                        channel.closed = true;
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                    }
                }
            }
        }
        self.outputs.retain(|channel| !channel.closed);
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
