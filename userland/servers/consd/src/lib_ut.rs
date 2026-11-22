// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the console multiplexer.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use super::*;
use raven_tty_core::{Mode, Settings};

#[derive(Default)]
struct InputState {
    bytes: VecDeque<u8>,
    closed: bool,
}

struct MockInput {
    state: Arc<Mutex<InputState>>,
}

impl MockInput {
    fn new(bytes: &[u8]) -> (Box<Self>, Arc<Mutex<InputState>>) {
        let state = Arc::new(Mutex::new(InputState {
            bytes: bytes.iter().copied().collect(),
            closed: false,
        }));
        (
            Box::new(Self {
                state: Arc::clone(&state),
            }),
            state,
        )
    }
}

impl ConsoleInput for MockInput {
    fn read(&mut self, out: &mut [u8]) -> Result<usize, InputError> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(InputError::EndOfFile);
        }
        if state.bytes.is_empty() {
            return Err(InputError::WouldBlock);
        }
        let count = out.len().min(state.bytes.len());
        for slot in out.iter_mut().take(count) {
            *slot = state.bytes.pop_front().unwrap_or(0);
        }
        Ok(count)
    }
}

#[derive(Default)]
struct OutputState {
    log: Vec<u8>,
    capacity: usize,
    closed: bool,
}

struct MockOutput {
    state: Arc<Mutex<OutputState>>,
}

impl MockOutput {
    fn new(capacity: usize) -> (Box<Self>, Arc<Mutex<OutputState>>) {
        let state = Arc::new(Mutex::new(OutputState {
            log: Vec::new(),
            capacity,
            closed: false,
        }));
        (
            Box::new(Self {
                state: Arc::clone(&state),
            }),
            state,
        )
    }
}

impl ConsoleOutput for MockOutput {
    fn write(&mut self, bytes: &[u8]) -> Result<usize, OutputError> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(OutputError::Closed);
        }
        if state.capacity == 0 {
            return Err(OutputError::Full);
        }
        let count = bytes.len().min(state.capacity);
        state.log.extend_from_slice(&bytes[..count]);
        state.capacity -= count;
        Ok(count)
    }
}

fn take(multiplexer: &mut Multiplexer, len: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; len];
    let count = multiplexer.take(&mut buffer);
    buffer.truncate(count);
    buffer
}

#[test]
fn uart_only_echoes_and_delivers_a_canonical_line() {
    let mut console = Multiplexer::new();
    let (input, _input_state) = MockInput::new(b"hi\n");
    console.add_input(input);
    let (output, state) = MockOutput::new(64);
    console.add_output(output);

    assert_eq!(console.pump(), 3);
    assert_eq!(take(&mut console, 8), b"hi\n");
    assert_eq!(state.lock().unwrap().log, b"hi\r\n");
}

#[test]
fn keyboard_only_is_the_same_line_source() {
    let mut console = Multiplexer::new();
    let (input, _input_state) = MockInput::new(b"kbd\n");
    console.add_input(input);
    let (output, state) = MockOutput::new(64);
    console.add_output(output);

    console.pump();
    assert_eq!(take(&mut console, 8), b"kbd\n");
    assert_eq!(state.lock().unwrap().log, b"kbd\r\n");
}

#[test]
fn both_inputs_feed_one_discipline_in_source_order() {
    let mut console = Multiplexer::new();
    let (uart, _uart_state) = MockInput::new(b"a\n");
    let (keyboard, _keyboard_state) = MockInput::new(b"b\n");
    console.add_input(uart);
    console.add_input(keyboard);
    let (output, state) = MockOutput::new(64);
    console.add_output(output);

    console.pump();
    assert_eq!(take(&mut console, 8), b"a\nb\n");
    assert_eq!(state.lock().unwrap().log, b"a\r\nb\r\n");
}

#[test]
fn canonical_echo_backspace_and_raw_mode_are_preserved() {
    let mut console = Multiplexer::new();
    console.feed(b"ab\x08\n");
    assert_eq!(take(&mut console, 8), b"a\n");

    let discarded = console.set_settings(Settings {
        mode: Mode::Raw,
        echo: true,
    });
    assert_eq!(discarded, 0);
    console.feed(b"raw");
    assert_eq!(take(&mut console, 8), b"raw");
}

#[test]
fn output_is_fanned_out_once_per_sink() {
    let mut console = Multiplexer::new();
    let (uart, uart_state) = MockOutput::new(64);
    let (framebuffer, framebuffer_state) = MockOutput::new(64);
    console.add_output(uart);
    console.add_output(framebuffer);

    assert_eq!(console.write_all(b"login: "), Ok(7));
    assert_eq!(uart_state.lock().unwrap().log, b"login: ");
    assert_eq!(framebuffer_state.lock().unwrap().log, b"login: ");
    assert_eq!(
        uart_state.lock().unwrap().log.len(),
        7,
        "no duplicate prompt"
    );
    assert_eq!(
        framebuffer_state.lock().unwrap().log.len(),
        7,
        "no duplicate prompt"
    );
}

#[test]
fn repeated_prompts_are_not_duplicated_by_fanout() {
    let mut console = Multiplexer::new();
    let (uart, uart_state) = MockOutput::new(64);
    let (framebuffer, framebuffer_state) = MockOutput::new(64);
    console.add_output(uart);
    console.add_output(framebuffer);

    console.write_all(b"login: ").unwrap();
    console.write_all(b"login: ").unwrap();
    assert_eq!(uart_state.lock().unwrap().log, b"login: login: ");
    assert_eq!(framebuffer_state.lock().unwrap().log, b"login: login: ");
}

#[test]
fn backpressure_retains_bytes_until_the_sink_drains() {
    let mut console = Multiplexer::new();
    let (output, state) = MockOutput::new(2);
    console.add_output(output);

    assert_eq!(console.write_all(b"abcdef"), Ok(6));
    assert_eq!(state.lock().unwrap().log, b"ab");
    state.lock().unwrap().capacity = 16;
    console.flush_outputs().unwrap();
    assert_eq!(state.lock().unwrap().log, b"abcdef");
}

#[test]
fn a_lost_output_is_retired_and_the_other_sink_stays_live() {
    let mut console = Multiplexer::new();
    let (lost, lost_state) = MockOutput::new(64);
    let (live, live_state) = MockOutput::new(64);
    console.add_output(lost);
    console.add_output(live);
    lost_state.lock().unwrap().closed = true;

    console.write_all(b"x").unwrap();
    console.flush_outputs().unwrap();
    assert_eq!(live_state.lock().unwrap().log, b"x");
    assert_eq!(console.output_count(), 1);
}

#[test]
fn a_lost_input_is_retired_and_eof_only_when_all_inputs_end() {
    let mut console = Multiplexer::new();
    let (uart, uart_state) = MockInput::new(b"");
    let (keyboard, keyboard_state) = MockInput::new(b"k\n");
    console.add_input(uart);
    console.add_input(keyboard);

    uart_state.lock().unwrap().closed = true;
    assert_eq!(console.pump(), 2);
    assert_eq!(take(&mut console, 8), b"k\n");
    assert!(!console.eof());
    assert_eq!(console.input_count(), 1);

    keyboard_state.lock().unwrap().closed = true;
    console.pump();
    assert!(console.eof());
}

#[test]
fn interrupt_and_flush_are_visible_to_the_scheme_reader() {
    let mut console = Multiplexer::new();
    console.feed(b"ab\x03");
    assert!(console.take_interrupt());
    assert!(!console.take_interrupt());
    console.feed(b"line");
    assert!(console.flush() >= 4);
    assert_eq!(take(&mut console, 8), b"");
}

#[test]
fn kbd_console_bytes_reach_the_console_reader() {
    // The `kbd:console` reader thread uses this same path: a scheme/mock source feeds the
    // multiplexer and the canonical line is what `tty:console` readers receive.
    let mut console = Multiplexer::new();
    let (keyboard, _keyboard_state) = MockInput::new(b"root\n");
    console.add_input(keyboard);
    let (output, state) = MockOutput::new(64);
    console.add_output(output);

    assert_eq!(console.pump(), 5);
    assert_eq!(take(&mut console, 8), b"root\n");
    assert_eq!(state.lock().unwrap().log, b"root\r\n");
}
