// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ufs-mkfs` command line.

#![forbid(unsafe_code)]

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use ufs_core::mkfs::MkfsOptions;
use ufs_mkfs::format_image;

const USAGE: &str = "\
usage: ufs-mkfs [-O 1|2] [-b block-size] [-f fragment-size] [-s size]
                [-d host-dir] [-T epoch] image

  -O  UFS version (default 2)
  -b  block size (default 4096)
  -f  fragment size (default 512)
  -s  image size, with optional k/m/g suffix (default: existing file size)
  -d  populate from this host directory
  -T  timestamp (default: SOURCE_DATE_EPOCH or the current time)
";

fn value<T: std::str::FromStr>(
    args: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<T, String> {
    *index += 1;
    let text = args
        .get(*index)
        .ok_or_else(|| format!("{flag} needs a value"))?;
    text.parse::<T>()
        .map_err(|_| format!("bad value for {flag}: {text}"))
}

fn size(text: &str) -> Result<u64, String> {
    let (digits, multiplier) = match text.as_bytes().last().copied() {
        Some(b'k' | b'K') => (&text[..text.len() - 1], 1024u64),
        Some(b'm' | b'M') => (&text[..text.len() - 1], 1024 * 1024),
        Some(b'g' | b'G') => (&text[..text.len() - 1], 1024 * 1024 * 1024),
        _ => (text, 1),
    };
    digits
        .parse::<u64>()
        .ok()
        .and_then(|value| value.checked_mul(multiplier))
        .ok_or_else(|| format!("bad size: {text}"))
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ufs-mkfs: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    let mut version = 2u8;
    let mut block_size = 4096u32;
    let mut fragment_size = 512u32;
    let mut size_bytes = 0u64;
    let mut host = None;
    let mut timestamp = None;
    let mut image = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-O" => version = value(&args, &mut index, "-O")?,
            "-b" => block_size = value(&args, &mut index, "-b")?,
            "-f" => fragment_size = value(&args, &mut index, "-f")?,
            "-s" => {
                index += 1;
                let text = args.get(index).ok_or("-s needs a size")?;
                size_bytes = size(text)?;
            }
            "-d" => {
                index += 1;
                host = Some(PathBuf::from(
                    args.get(index).ok_or("-d needs a directory")?,
                ));
            }
            "-T" => timestamp = Some(value(&args, &mut index, "-T")?),
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            _ => {
                if image.is_some() {
                    return Err("only one image path is accepted".to_string());
                }
                image = Some(PathBuf::from(&args[index]));
            }
        }
        index += 1;
    }
    let image = image.ok_or("missing image path")?;
    let timestamp = timestamp.unwrap_or_else(|| {
        env::var("SOURCE_DATE_EPOCH")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_else(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|duration| duration.as_secs() as i64)
                    .unwrap_or(0)
            })
    });
    let opts = MkfsOptions {
        size: size_bytes,
        version,
        block_size,
        fragment_size,
        bytes_per_inode: 4096,
        minfree: 5,
        label: "",
        timestamp,
    };
    format_image(&image, &opts, host.as_deref())
}
