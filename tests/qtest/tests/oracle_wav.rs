// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! WAV oracle tests using generated files.
//!
//! The files are built byte by byte here; no external tools or fixtures are needed. Every
//! parser and analyser failure must be a `Result`/report, never a panic.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use raven_qtest::{Case, OracleCheck, OracleReport, OracleSpec, analyze_oracle, parse_wav};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "raven-qtest-oracle-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, bytes).expect("write test artifact");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Built {
    bytes: Vec<u8>,
    data_size_offset: usize,
}

fn build_with(
    rate: u32,
    channels: u16,
    bits: u16,
    samples: &[i16],
    fmt_extension: usize,
    before_fmt: &[(&[u8; 4], &[u8])],
    after_data: &[(&[u8; 4], &[u8])],
) -> Built {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    for (id, body) in before_fmt {
        push_chunk(&mut bytes, id, body);
    }
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&rate.to_le_bytes());
    let block_align = channels.saturating_mul(bits / 8);
    fmt.extend_from_slice(&(rate.saturating_mul(u32::from(block_align))).to_le_bytes());
    fmt.extend_from_slice(&block_align.to_le_bytes());
    fmt.extend_from_slice(&bits.to_le_bytes());
    fmt.resize(16 + fmt_extension, 0);
    push_chunk(&mut bytes, b"fmt ", &fmt);
    bytes.extend_from_slice(b"data");
    let data_size_offset = bytes.len();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    for (id, body) in after_data {
        push_chunk(&mut bytes, id, body);
    }
    let riff_size = u32::try_from(bytes.len().saturating_sub(8)).unwrap_or(u32::MAX);
    bytes[4..8].copy_from_slice(&riff_size.to_le_bytes());
    Built {
        bytes,
        data_size_offset,
    }
}

fn build(rate: u32, channels: u16, samples: &[i16]) -> Built {
    build_with(rate, channels, 16, samples, 0, &[], &[])
}

fn push_chunk(bytes: &mut Vec<u8>, id: &[u8; 4], body: &[u8]) {
    bytes.extend_from_slice(id);
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(body);
    if !body.len().is_multiple_of(2) {
        bytes.push(0);
    }
}

fn patch_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn tone(rate: u32, channels: u16, hz: f64, duration_ms: u32) -> Vec<i16> {
    let frames = (f64::from(rate) * f64::from(duration_ms) / 1000.0).round() as usize;
    let mut samples = Vec::with_capacity(frames * usize::from(channels));
    for frame in 0..frames {
        let phase = std::f64::consts::TAU * hz * frame as f64 / f64::from(rate);
        let value = (0.5 * 32767.0 * phase.sin()) as i16;
        for _ in 0..channels {
            samples.push(value);
        }
    }
    samples
}

fn silence(rate: u32, channels: u16, duration_ms: u32) -> Vec<i16> {
    let frames = (f64::from(rate) * f64::from(duration_ms) / 1000.0).round() as usize;
    vec![0; frames * usize::from(channels)]
}

fn spec(path: &Path) -> OracleSpec {
    OracleSpec {
        kind: "wav".to_string(),
        path: path.display().to_string(),
        min_duration_ms: None,
        min_rms: None,
        tone_hz: None,
        tolerance_percent: None,
        max_gap_ms: None,
    }
}

fn check<'a>(report: &'a OracleReport, name: &str) -> &'a OracleCheck {
    report
        .checks
        .iter()
        .find(|check| check.check == name)
        .unwrap_or_else(|| panic!("missing check {name}"))
}

fn analyse(temp: &TempDir, name: &str, built: &Built, spec: &mut OracleSpec) -> OracleReport {
    let path = temp.write(name, &built.bytes);
    spec.path = path.display().to_string();
    analyze_oracle(spec)
}

#[test]
fn mono_44100_tone_passes() {
    let temp = TempDir::new("mono");
    let built = build(44_100, 1, &tone(44_100, 1, 440.0, 500));
    let mut spec = spec(Path::new("unused"));
    spec.min_duration_ms = Some(400);
    spec.min_rms = Some(0.1);
    spec.tone_hz = Some(440.0);
    spec.tolerance_percent = Some(2.0);
    let report = analyse(&temp, "tone.wav", &built, &mut spec);
    assert!(report.passed(), "{report:?}");
    assert!(check(&report, "min_duration_ms").passed);
    assert!(check(&report, "min_rms").passed);
    assert!(check(&report, "tone_hz").passed);
}

#[test]
fn stereo_48000_tone_passes() {
    let temp = TempDir::new("stereo");
    let built = build(48_000, 2, &tone(48_000, 2, 440.0, 500));
    let mut spec = spec(Path::new("unused"));
    spec.tone_hz = Some(440.0);
    spec.tolerance_percent = Some(2.0);
    let report = analyse(&temp, "tone.wav", &built, &mut spec);
    assert!(report.passed(), "{report:?}");
    assert!(check(&report, "tone_hz").passed);
}

#[test]
fn silence_fails_min_rms() {
    let temp = TempDir::new("silence");
    let built = build(48_000, 1, &silence(48_000, 1, 200));
    let mut spec = spec(Path::new("unused"));
    spec.min_rms = Some(0.01);
    let report = analyse(&temp, "silence.wav", &built, &mut spec);
    assert!(!report.passed());
    assert!(!check(&report, "min_rms").passed);
}

#[test]
fn wrong_tone_fails() {
    let temp = TempDir::new("wrong");
    let built = build(48_000, 1, &tone(48_000, 1, 880.0, 500));
    let mut spec = spec(Path::new("unused"));
    spec.tone_hz = Some(440.0);
    spec.tolerance_percent = Some(2.0);
    let report = analyse(&temp, "wrong.wav", &built, &mut spec);
    assert!(!report.passed());
    assert!(!check(&report, "tone_hz").passed);
}

#[test]
fn tolerance_boundary_passes_and_fails() {
    let temp = TempDir::new("tolerance");
    let pass = build(44_100, 1, &tone(44_100, 1, 445.0, 500));
    let mut spec = spec(Path::new("unused"));
    spec.tone_hz = Some(440.0);
    spec.tolerance_percent = Some(2.0);
    let report = analyse(&temp, "inside.wav", &pass, &mut spec);
    assert!(report.passed(), "{report:?}");

    let fail = build(44_100, 1, &tone(44_100, 1, 450.0, 500));
    let report = analyse(&temp, "outside.wav", &fail, &mut spec);
    assert!(!report.passed());
    assert!(!check(&report, "tone_hz").passed);
}

#[test]
fn duration_check_uses_milliseconds() {
    let temp = TempDir::new("duration");
    let built = build(48_000, 1, &tone(48_000, 1, 440.0, 100));
    let mut spec = spec(Path::new("unused"));
    spec.min_duration_ms = Some(90);
    let report = analyse(&temp, "long.wav", &built, &mut spec);
    assert!(check(&report, "min_duration_ms").passed);

    spec.min_duration_ms = Some(110);
    let report = analyse(&temp, "short.wav", &built, &mut spec);
    assert!(!check(&report, "min_duration_ms").passed);
}

#[test]
fn gap_within_limit_passes_and_over_limit_fails() {
    let temp = TempDir::new("gap");
    let mut samples = tone(48_000, 1, 440.0, 200);
    samples.extend(silence(48_000, 1, 100));
    samples.extend(tone(48_000, 1, 440.0, 200));
    let built = build(48_000, 1, &samples);
    let mut spec = spec(Path::new("unused"));
    spec.max_gap_ms = Some(120);
    let report = analyse(&temp, "gap-ok.wav", &built, &mut spec);
    assert!(check(&report, "max_gap_ms").passed, "{report:?}");

    spec.max_gap_ms = Some(50);
    let report = analyse(&temp, "gap-bad.wav", &built, &mut spec);
    assert!(!check(&report, "max_gap_ms").passed, "{report:?}");
}

#[test]
fn leading_silence_is_ignored_for_gap() {
    let temp = TempDir::new("lead");
    let mut samples = silence(48_000, 1, 100);
    samples.extend(tone(48_000, 1, 440.0, 200));
    let built = build(48_000, 1, &samples);
    let mut spec = spec(Path::new("unused"));
    spec.max_gap_ms = Some(50);
    let report = analyse(&temp, "lead.wav", &built, &mut spec);
    assert!(check(&report, "max_gap_ms").passed, "{report:?}");
}

#[test]
fn zero_data_size_uses_file_length() {
    let temp = TempDir::new("data-zero");
    let samples = tone(48_000, 1, 440.0, 200);
    let mut built = build(48_000, 1, &samples);
    patch_u32(&mut built.bytes, built.data_size_offset, 0);
    let mut spec = spec(Path::new("unused"));
    spec.min_duration_ms = Some(150);
    let report = analyse(&temp, "zero.wav", &built, &mut spec);
    assert!(report.passed(), "{report:?}");
}

#[test]
fn ffffffff_data_size_uses_file_length() {
    let temp = TempDir::new("data-ff");
    let samples = tone(48_000, 2, 440.0, 200);
    let mut built = build(48_000, 2, &samples);
    patch_u32(&mut built.bytes, built.data_size_offset, u32::MAX);
    let mut spec = spec(Path::new("unused"));
    spec.tone_hz = Some(440.0);
    spec.tolerance_percent = Some(2.0);
    let report = analyse(&temp, "ff.wav", &built, &mut spec);
    assert!(report.passed(), "{report:?}");
}

#[test]
fn unknown_chunks_are_skipped() {
    let temp = TempDir::new("chunks");
    let samples = tone(44_100, 1, 440.0, 200);
    let before = [(b"JUNK", b"abc".as_slice())];
    let after = [(b"LIST", b"INFOxxxx".as_slice())];
    let built = build_with(44_100, 1, 16, &samples, 0, &before, &after);
    let mut spec = spec(Path::new("unused"));
    spec.tone_hz = Some(440.0);
    spec.tolerance_percent = Some(2.0);
    let report = analyse(&temp, "chunks.wav", &built, &mut spec);
    assert!(report.passed(), "{report:?}");
}

#[test]
fn odd_sized_chunk_padding_is_skipped() {
    let temp = TempDir::new("odd");
    let samples = tone(44_100, 1, 440.0, 200);
    let before = [(b"JUNK", b"odd".as_slice())];
    let built = build_with(44_100, 1, 16, &samples, 0, &before, &[]);
    let mut spec = spec(Path::new("unused"));
    spec.min_duration_ms = Some(150);
    let report = analyse(&temp, "odd.wav", &built, &mut spec);
    assert!(report.passed(), "{report:?}");
}

#[test]
fn extended_fmt_chunk_is_accepted() {
    let temp = TempDir::new("fmt-ext");
    let samples = tone(48_000, 1, 440.0, 200);
    let built = build_with(48_000, 1, 16, &samples, 2, &[], &[]);
    let mut spec = spec(Path::new("unused"));
    spec.min_duration_ms = Some(150);
    let report = analyse(&temp, "fmt.wav", &built, &mut spec);
    assert!(report.passed(), "{report:?}");
}

#[test]
fn truncated_header_fails() {
    let temp = TempDir::new("trunc-header");
    let built = build(48_000, 1, &tone(48_000, 1, 440.0, 200));
    let path = temp.write("short.wav", &built.bytes[..10]);
    let report = analyze_oracle(&spec(&path));
    assert!(!report.passed());
    assert!(report.error.is_some());
}

#[test]
fn truncated_data_fails() {
    let temp = TempDir::new("trunc-data");
    let mut built = build(48_000, 1, &tone(48_000, 1, 440.0, 200));
    patch_u32(&mut built.bytes, built.data_size_offset, 1_000_000);
    let path = temp.write("truncated.wav", &built.bytes);
    let report = analyze_oracle(&spec(&path));
    assert!(!report.passed());
    assert!(report.error.unwrap().contains("truncated"));
}

#[test]
fn bad_riff_magic_fails() {
    let temp = TempDir::new("bad-magic");
    let mut built = build(48_000, 1, &tone(48_000, 1, 440.0, 200));
    built.bytes[0] = b'X';
    let path = temp.write("bad.wav", &built.bytes);
    let report = analyze_oracle(&spec(&path));
    assert!(!report.passed());
    assert!(report.error.unwrap().contains("RIFF"));
}

#[test]
fn unsupported_sample_width_fails() {
    let temp = TempDir::new("bits");
    let built = build_with(48_000, 1, 8, &tone(48_000, 1, 440.0, 200), 0, &[], &[]);
    let path = temp.write("bits.wav", &built.bytes);
    let report = analyze_oracle(&spec(&path));
    assert!(!report.passed());
    assert!(report.error.unwrap().contains("PCM16"));
}

#[test]
fn unsupported_channel_count_fails() {
    let temp = TempDir::new("channels");
    let samples = vec![0i16; 1000];
    let built = build_with(48_000, 3, 16, &samples, 0, &[], &[]);
    let path = temp.write("channels.wav", &built.bytes);
    let report = analyze_oracle(&spec(&path));
    assert!(!report.passed());
    assert!(report.error.unwrap().contains("channel"));
}

#[test]
fn missing_data_chunk_fails() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&48_000u32.to_le_bytes());
    bytes.extend_from_slice(&96_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    let size = u32::try_from(bytes.len() - 8).unwrap();
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    let temp = TempDir::new("no-data");
    let path = temp.write("nodata.wav", &bytes);
    let report = analyze_oracle(&spec(&path));
    assert!(!report.passed());
    assert!(report.error.unwrap().contains("data"));
}

#[test]
fn missing_file_reports_a_failure() {
    let report = analyze_oracle(&spec(Path::new("/not/a/real/audio.wav")));
    assert!(!report.passed());
    assert!(report.checks.is_empty());
    assert!(report.error.is_some());
}

#[test]
fn malformed_inputs_never_panic() {
    let built = build(44_100, 2, &tone(44_100, 2, 440.0, 100));
    for length in 0..built.bytes.len() {
        let _ = parse_wav(&built.bytes[..length]);
    }
    let mut state = 0xdead_beef_1234_5678u64;
    for _ in 0..1000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let length = (state as usize) % 200;
        let mut bytes = vec![0u8; length];
        bytes.fill((state >> 8) as u8);
        let _ = parse_wav(&bytes);
    }
}

#[test]
fn case_parser_accepts_an_oracle_section() {
    let text = r#"
name = "audio"
arch = "x86_64"
profile = "base"
timeout_secs = 20
expect_exit = "success"
qemu_extra = ["-audiodev", "wav,id=a0,path=build/qtest/audio.wav"]
[[oracle]]
kind = "wav"
path = "build/qtest/audio.wav"
min_duration_ms = 500
min_rms = 0.01
tone_hz = 440.0
tolerance_percent = 2.0
max_gap_ms = 50
"#;
    let case = Case::parse(text).unwrap();
    assert_eq!(case.oracles.len(), 1);
    assert_eq!(case.oracles[0].tone_hz, Some(440.0));
}

#[test]
fn case_parser_rejects_bad_oracle_sections() {
    let base = "name = \"audio\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 20\n\
                expect_exit = \"success\"\n";
    for oracle in [
        "[[oracle]]\nkind = \"png\"\npath = \"x\"\n",
        "[[oracle]]\nkind = \"wav\"\n",
        "[[oracle]]\nkind = \"wav\"\npath = \"x\"\ntone_hz = 440.0\n",
        "[[oracle]]\nkind = \"wav\"\npath = \"x\"\nmystery = 1\n",
        "[[oracle]]\nkind = \"wav\"\npath = \"x\"\nmin_rms = -1.0\n",
    ] {
        assert!(Case::parse(&format!("{base}{oracle}")).is_err(), "{oracle}");
    }
}

#[test]
fn case_without_oracles_is_unchanged() {
    let text = "name = \"plain\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 20\n\
                expect_exit = \"success\"\n";
    let case = Case::parse(text).unwrap();
    assert!(case.oracles.is_empty());
}

#[test]
fn file_check_is_reported_when_no_checks_are_requested() {
    let temp = TempDir::new("file-check");
    let built = build(48_000, 1, &silence(48_000, 1, 100));
    let mut spec = spec(Path::new("unused"));
    let report = analyse(&temp, "valid.wav", &built, &mut spec);
    assert!(report.passed());
    assert_eq!(report.checks.len(), 1);
    assert_eq!(report.checks[0].check, "file");
}

#[test]
fn oracle_sidecar_and_cli_report_each_check() {
    let temp = TempDir::new("cli");
    let built = build(48_000, 2, &tone(48_000, 2, 440.0, 300));
    let wav = temp.write("good.wav", &built.bytes);
    let case_path = temp.path().join("audio.toml");
    let case = format!(
        "name = \"audio\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\n\
         [[oracle]]\nkind = \"wav\"\npath = \"{}\"\nmin_duration_ms = 200\n\
         tone_hz = 440.0\ntolerance_percent = 2.0\n",
        wav.display()
    );
    fs::write(&case_path, case).unwrap();
    let out = temp.path().join("out");
    let output = Command::new(env!("CARGO_BIN_EXE_raven-qtest"))
        .args(["run", "--case"])
        .arg(&case_path)
        .args(["--out"])
        .arg(&out)
        .args(["--runner", "sh", "-c", "exit 33"])
        .output()
        .expect("run qtest");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("audio: pass"), "{stdout}");
    assert!(stdout.contains("oracle "), "{stdout}");
    assert!(stdout.contains("tone_hz: pass"), "{stdout}");
    let sidecar = fs::read_to_string(out.join("audio.oracle.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sidecar).unwrap();
    assert_eq!(parsed["case"], "audio");
    assert_eq!(parsed["oracles"][0]["checks"][1]["check"], "tone_hz");
}

#[test]
fn cli_fails_the_case_when_the_oracle_fails() {
    let temp = TempDir::new("cli-fail");
    let built = build(48_000, 1, &tone(48_000, 1, 880.0, 300));
    let wav = temp.write("wrong.wav", &built.bytes);
    let case_path = temp.path().join("wrong.toml");
    let case = format!(
        "name = \"wrong\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\n\
         [[oracle]]\nkind = \"wav\"\npath = \"{}\"\n\
         tone_hz = 440.0\ntolerance_percent = 2.0\n",
        wav.display()
    );
    fs::write(&case_path, case).unwrap();
    let out = temp.path().join("out");
    let output = Command::new(env!("CARGO_BIN_EXE_raven-qtest"))
        .args(["run", "--case"])
        .arg(&case_path)
        .args(["--out"])
        .arg(&out)
        .args(["--runner", "sh", "-c", "exit 33"])
        .output()
        .expect("run qtest");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("oracle"), "{stderr}");
    let json = fs::read_to_string(out.join("wrong.json")).unwrap();
    assert!(json.contains("\"fail\""), "{json}");
}
