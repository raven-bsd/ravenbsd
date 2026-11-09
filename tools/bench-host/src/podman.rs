// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Podman container start time and idle memory, using only local images.

use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result};

use crate::result::BenchResult;

const RUNS: usize = 20;
const WARMUP: usize = 1;

pub(crate) fn run() -> Result<()> {
    let images = match Command::new("podman")
        .args(["images", "--format", "{{.Repository}}:{{.Tag}}"])
        .output()
    {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && *line != "<none>:<none>")
            .map(str::to_string)
            .collect::<Vec<_>>(),
        Ok(output) => {
            return emit_unmeasurable(format!(
                "not measurable: podman images failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Err(error) => {
            return emit_unmeasurable(format!("not measurable: cannot run podman: {error}"));
        }
    };
    let Some(image) = images
        .iter()
        .find(|image| image.as_str() == "docker.io/library/alpine:3.22")
        .or_else(|| images.first())
        .cloned()
    else {
        return emit_unmeasurable(String::from(
            "not measurable: no local image (podman images is empty)",
        ));
    };

    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..WARMUP {
        if let Err(error) = podman_true(&image) {
            return emit_unmeasurable(format!(
                "not measurable: podman run {image} /bin/true failed: {error:#}"
            ));
        }
    }
    for _ in 0..RUNS {
        let start = Instant::now();
        podman_true(&image).with_context(|| format!("podman run {image} /bin/true"))?;
        samples.push(start.elapsed().as_nanos() as f64);
    }
    let start = BenchResult::from_samples(
        "podman-start",
        "ns/op",
        &samples,
        Some(format!("image={image}; local image, --pull=never")),
    );
    start.print();
    start.write()?;

    match podman_memory(&image) {
        Ok(bytes) => {
            let memory = BenchResult::from_samples(
                "podman-rss",
                "bytes",
                &[bytes as f64],
                Some(format!("image={image}; podman stats on a sleep container")),
            );
            memory.print();
            memory.write()?;
        }
        Err(error) => {
            let memory = BenchResult::from_samples(
                "podman-rss",
                "bytes",
                &[],
                Some(format!("not measurable: {error:#}")),
            );
            memory.print();
            memory.write()?;
        }
    }
    Ok(())
}

fn podman_true(image: &str) -> Result<()> {
    let output = Command::new("podman")
        .args(["run", "--pull=never", "--rm", image, "/bin/true"])
        .output()
        .context("cannot run podman")?;
    if !output.status.success() {
        anyhow::bail!(
            "exit {}: {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn podman_memory(image: &str) -> Result<u64> {
    let started = Command::new("podman")
        .args(["run", "--pull=never", "--rm", "-d", image, "sleep", "30"])
        .output()
        .context("cannot run podman sleep container")?;
    if !started.status.success() {
        anyhow::bail!(
            "podman sleep failed: {}",
            String::from_utf8_lossy(&started.stderr).trim()
        );
    }
    let id = String::from_utf8_lossy(&started.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if id.is_empty() {
        anyhow::bail!("podman did not print a container id");
    }

    let stats = Command::new("podman")
        .args([
            "stats",
            "--no-stream",
            "--format",
            "{{.MemUsage}}",
            id.as_str(),
        ])
        .output()
        .context("cannot run podman stats");
    let cleanup = Command::new("podman")
        .args(["rm", "-f", id.as_str()])
        .output();
    let stats = stats?;
    let _ = cleanup;
    if !stats.status.success() {
        anyhow::bail!(
            "podman stats failed: {}",
            String::from_utf8_lossy(&stats.stderr).trim()
        );
    }
    let usage = String::from_utf8_lossy(&stats.stdout);
    let first = usage.split('/').next().unwrap_or("").trim();
    parse_size(first).with_context(|| format!("cannot parse podman memory `{first}`"))
}

fn parse_size(text: &str) -> Result<u64> {
    let split = text
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(text.len());
    let number: f64 = text[..split].trim().parse().context("number")?;
    let suffix = text[split..].trim().to_ascii_uppercase();
    let multiplier = match suffix.as_str() {
        "" | "B" => 1.0,
        "KB" | "K" => 1_000.0,
        "MB" | "M" => 1_000_000.0,
        "GB" | "G" => 1_000_000_000.0,
        "KIB" => 1024.0,
        "MIB" => 1024.0 * 1024.0,
        "GIB" => 1024.0 * 1024.0 * 1024.0,
        _ => anyhow::bail!("unknown size suffix `{suffix}`"),
    };
    Ok((number * multiplier) as u64)
}

fn emit_unmeasurable(note: String) -> Result<()> {
    let result = BenchResult::from_samples("podman-start", "ns/op", &[], Some(note));
    result.print();
    result.write()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_size;

    #[test]
    fn parses_podman_memory_units() {
        assert_eq!(parse_size("1B").unwrap(), 1);
        assert_eq!(parse_size("1.5MB").unwrap(), 1_500_000);
        assert_eq!(parse_size("2MiB").unwrap(), 2 * 1024 * 1024);
        assert!(parse_size("12XB").is_err());
    }
}
