// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ufs-inject` command line.
//!
//! The image path is followed by a command; global `--offset`/`--size` select a UFS partition
//! inside a whole-disk image or block device.

#![forbid(unsafe_code)]

use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use ufs_inject::{
    Image, MkdirOptions, PutOptions, SymlinkOptions, cat, fsck, list, mkdir, put, stat, symlink,
    type_name,
};

const USAGE: &str = "\
usage: ufs-inject [--offset BYTES] [--size BYTES] IMAGE COMMAND ...

  put HOST DEST [--mode OCTAL] [--uid N] [--gid N] [--parents]
  mkdir PATH [--mode OCTAL] [--uid N] [--gid N] [--parents]
  symlink TARGET PATH [--uid N] [--gid N] [--parents]
  cat PATH
  list PATH
  stat PATH
  check

  --offset BYTES  UFS partition byte offset inside IMAGE (default 0)
  --size BYTES    UFS partition length (default: to the end of the file)
";

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("ufs-inject: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<u8, String> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{USAGE}");
        return Ok(0);
    }
    let mut offset = 0u64;
    let mut size = 0u64;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--offset" => {
                index += 1;
                offset = parse_u64(args.get(index).ok_or("--offset needs a value")?)?;
            }
            "--size" => {
                index += 1;
                size = parse_u64(args.get(index).ok_or("--size needs a value")?)?;
            }
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            _ => break,
        }
        index += 1;
    }
    let image_path = args.get(index).ok_or("missing image path")?;
    index += 1;
    let command = args.get(index).ok_or("missing command")?;
    index += 1;
    let rest = &args[index..];
    let image = Image::new(PathBuf::from(image_path), offset, size);
    match command.as_str() {
        "put" => command_put(&image, rest),
        "mkdir" => command_mkdir(&image, rest),
        "symlink" => command_symlink(&image, rest),
        "cat" => command_cat(&image, rest),
        "list" => command_list(&image, rest),
        "stat" => command_stat(&image, rest),
        "check" => command_check(&image, rest),
        other => Err(format!("unknown command {other}")),
    }
}

fn command_put(image: &Image, args: &[String]) -> Result<u8, String> {
    if args.len() < 2 {
        return Err(String::from("put needs HOST and DEST"));
    }
    let host = PathBuf::from(&args[0]);
    let dest = args[1].as_str();
    let mut options = PutOptions::default();
    let mut index = 2usize;
    while index < args.len() {
        match args[index].as_str() {
            "--mode" => {
                index += 1;
                options.mode = Some(parse_mode(args.get(index).ok_or("--mode needs a value")?)?);
            }
            "--uid" => {
                index += 1;
                options.uid = Some(parse_u32(args.get(index).ok_or("--uid needs a value")?)?);
            }
            "--gid" => {
                index += 1;
                options.gid = Some(parse_u32(args.get(index).ok_or("--gid needs a value")?)?);
            }
            "--parents" => options.parents = true,
            other => return Err(format!("unknown put option {other}")),
        }
        index += 1;
    }
    let report = put(image, &host, dest, &options)?;
    println!(
        "ufs-inject: put {dest} ino={} replaced={} bytes={} fsck=clean",
        report.ino, report.replaced, report.bytes
    );
    Ok(0)
}

fn command_mkdir(image: &Image, args: &[String]) -> Result<u8, String> {
    let path = args.first().ok_or("mkdir needs PATH")?;
    let mut options = MkdirOptions::default();
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--mode" => {
                index += 1;
                options.mode = Some(parse_mode(args.get(index).ok_or("--mode needs a value")?)?);
            }
            "--uid" => {
                index += 1;
                options.uid = Some(parse_u32(args.get(index).ok_or("--uid needs a value")?)?);
            }
            "--gid" => {
                index += 1;
                options.gid = Some(parse_u32(args.get(index).ok_or("--gid needs a value")?)?);
            }
            "--parents" => options.parents = true,
            other => return Err(format!("unknown mkdir option {other}")),
        }
        index += 1;
    }
    let ino = mkdir(image, path, &options)?;
    println!("ufs-inject: mkdir {path} ino={ino} fsck=clean");
    Ok(0)
}

fn command_symlink(image: &Image, args: &[String]) -> Result<u8, String> {
    if args.len() < 2 {
        return Err(String::from("symlink needs TARGET and PATH"));
    }
    let target = args[0].as_str();
    let path = args[1].as_str();
    let mut options = SymlinkOptions::default();
    let mut index = 2usize;
    while index < args.len() {
        match args[index].as_str() {
            "--uid" => {
                index += 1;
                options.uid = Some(parse_u32(args.get(index).ok_or("--uid needs a value")?)?);
            }
            "--gid" => {
                index += 1;
                options.gid = Some(parse_u32(args.get(index).ok_or("--gid needs a value")?)?);
            }
            "--parents" => options.parents = true,
            other => return Err(format!("unknown symlink option {other}")),
        }
        index += 1;
    }
    let ino = symlink(image, target, path, &options)?;
    println!("ufs-inject: symlink {path} -> {target} ino={ino} fsck=clean");
    Ok(0)
}

fn command_cat(image: &Image, args: &[String]) -> Result<u8, String> {
    let path = args.first().ok_or("cat needs PATH")?;
    if args.len() != 1 {
        return Err(String::from("cat takes exactly one PATH"));
    }
    let data = cat(image, path)?;
    let mut stdout = std::io::stdout();
    stdout
        .write_all(&data)
        .map_err(|error| format!("stdout: {error}"))?;
    Ok(0)
}

fn command_list(image: &Image, args: &[String]) -> Result<u8, String> {
    let path = args.first().ok_or("list needs PATH")?;
    if args.len() != 1 {
        return Err(String::from("list takes exactly one PATH"));
    }
    for name in list(image, path)? {
        println!("{name}");
    }
    Ok(0)
}

fn command_stat(image: &Image, args: &[String]) -> Result<u8, String> {
    let path = args.first().ok_or("stat needs PATH")?;
    if args.len() != 1 {
        return Err(String::from("stat takes exactly one PATH"));
    }
    let attr = stat(image, path)?;
    println!(
        "{} mode={:#o} uid={} gid={} size={} ino={} nlink={}",
        type_name(attr.kind),
        attr.mode,
        attr.uid,
        attr.gid,
        attr.size,
        attr.ino,
        attr.nlink
    );
    Ok(0)
}

fn command_check(image: &Image, args: &[String]) -> Result<u8, String> {
    if !args.is_empty() {
        return Err(String::from("check takes no arguments"));
    }
    let report = fsck(image)?;
    for finding in &report.findings {
        let mut line = format!("phase {}: {:?}", finding.phase, finding.problem);
        if let Some(inode) = finding.inode {
            line.push_str(&format!(" inode={inode}"));
        }
        if let Some(block) = finding.block {
            line.push_str(&format!(" block={block}"));
        }
        println!("{line}");
    }
    if report.clean_after {
        println!("{}: clean", image.path.display());
        Ok(0)
    } else {
        println!(
            "{}: {} unresolved finding(s)",
            image.path.display(),
            report.findings.len()
        );
        Ok(4)
    }
}

fn parse_u64(text: &str) -> Result<u64, String> {
    if let Some(hex) = text.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).map_err(|_| format!("bad number {text}"))
    } else {
        text.parse().map_err(|_| format!("bad number {text}"))
    }
}

fn parse_u32(text: &str) -> Result<u32, String> {
    text.parse().map_err(|_| format!("bad number {text}"))
}

fn parse_mode(text: &str) -> Result<u16, String> {
    let digits = text.strip_prefix("0o").unwrap_or(text);
    let mode = u16::from_str_radix(digits, 8).map_err(|_| format!("bad mode {text}"))?;
    if mode > 0o7777 {
        return Err(format!("mode {text} exceeds 0o7777"));
    }
    Ok(mode)
}
