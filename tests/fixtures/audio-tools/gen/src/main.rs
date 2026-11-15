// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Generate the B45 `tone440.wav` fixture on the host.

use std::env;
use std::fs;

use raven_audio_core::tone::{Tone, Waveform};
use raven_audio_core::wav::{Wav, encode};

const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
const FRAMES: usize = 24_000; // 0.5 s

fn main() {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: audio-tools-gen <out.wav>");
        std::process::exit(2);
    };
    let Some(mut tone) = Tone::new(Waveform::Sine, RATE, 440, 8_000) else {
        eprintln!("cannot build the tone");
        std::process::exit(1);
    };
    let mut samples = Vec::with_capacity(FRAMES * usize::from(CHANNELS));
    for _ in 0..FRAMES {
        let sample = tone.next_sample();
        samples.push(sample);
        samples.push(sample);
    }
    let wav = Wav {
        rate: RATE,
        channels: CHANNELS,
        samples,
    };
    let Some(bytes) = encode(&wav) else {
        eprintln!("cannot encode the WAV");
        std::process::exit(1);
    };
    if let Some(parent) = std::path::Path::new(&path).parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(&path, bytes).expect("write");
    println!("audio-tools-gen: {path}");
}
