// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-flash`: safe removable-media image writer.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};

use raven_flash::cli::{self, Options};
use raven_flash::device::{Candidate, Device, FilterConfig, enumerate};
use raven_flash::image::{ImageInfo, inspect};
use raven_flash::writer::{unmount, validate_pair, write_and_verify};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("raven-flash: {error:#}");
            ExitCode::from(1)
        }
    }
}

fn run(args: &[String]) -> Result<u8> {
    if args.is_empty() && std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        raven_flash::tui::run(FilterConfig::default())?;
        return Ok(0);
    }
    let options = cli::parse(args)?;
    if options.help {
        println!("{}", cli::usage());
        return Ok(0);
    }
    let config = FilterConfig {
        sys_root: options.sys_root.clone(),
        mounts: options.mounts.clone(),
        dev_root: PathBuf::from("/dev"),
        root_device: options.root_device.clone(),
    };
    if options.helper_write {
        return helper_write(&options, &config);
    }
    if options.list {
        list(&options, &config)?;
        return Ok(0);
    }
    if options.dry_run {
        dry_run(&options, &config)?;
        return Ok(0);
    }
    if options.yes {
        return direct_write(&options, &config);
    }
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        raven_flash::tui::run(config)?;
        return Ok(0);
    }
    bail!(
        "non-interactive mode needs --list, --dry-run or --yes\n{}",
        cli::usage()
    )
}

fn list(options: &Options, config: &FilterConfig) -> Result<()> {
    let image = image_option(options)?;
    let candidates = enumerate(config, image.as_ref())?;
    if candidates.is_empty() {
        println!("no block devices found");
        return Ok(());
    }
    for candidate in candidates {
        match candidate.reject {
            Some(reason) => println!("REFUSED {}: {reason}", candidate.device.summary()),
            None => println!("SAFE    {}", candidate.device.summary()),
        }
    }
    Ok(())
}

fn dry_run(options: &Options, config: &FilterConfig) -> Result<()> {
    let path = options
        .image
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--dry-run needs --image IMG"))?;
    let image = inspect(path)?;
    println!("image: {}", image.summary());
    let candidates = enumerate(config, Some(&image))?;
    if let Some(device) = &options.device {
        let candidate = find_candidate(&candidates, device)?;
        if let Some(reason) = &candidate.reject {
            bail!("refusing {}: {reason}", candidate.device.summary());
        }
        if !candidate.device.mounted.is_empty() {
            println!(
                "would unmount: {}",
                candidate
                    .device
                    .mounted
                    .iter()
                    .map(|mount| mount.target.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        println!(
            "dry-run: {} -> {}",
            image.path.display(),
            candidate.device.path.display()
        );
        println!("dry-run: no bytes written");
        return Ok(());
    }
    println!("dry-run: no target selected");
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.reject.is_none())
    {
        println!("candidate: {}", candidate.device.summary());
    }
    Ok(())
}

fn direct_write(options: &Options, config: &FilterConfig) -> Result<u8> {
    let image = inspect(
        options
            .image
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--yes needs --image"))?,
    )?;
    let device_path = options
        .device
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--yes needs --device"))?;
    let candidates = enumerate(config, Some(&image))?;
    let candidate = find_candidate(&candidates, device_path)?.clone();
    if let Some(reason) = &candidate.reject {
        bail!("refusing {}: {reason}", candidate.device.summary());
    }
    if !candidate.device.mounted.is_empty() {
        bail!(
            "{} has mounted partitions; use the TUI to confirm the unmount",
            candidate.device.name
        );
    }
    validate_pair(&image, &candidate.device)?;
    if !is_root()? {
        return elevate(options, &candidate.device);
    }
    write_direct(&image, &candidate.device, &mut |done, total| {
        print!("\rwritten {done}/{total} bytes");
        use std::io::Write;
        let _ = std::io::stdout().flush();
    })?;
    Ok(0)
}

fn helper_write(options: &Options, config: &FilterConfig) -> Result<u8> {
    let image = inspect(
        options
            .image
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--helper-write needs --image"))?,
    )?;
    let device_path = options
        .device
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--helper-write needs --device"))?;
    let confirm = options
        .confirm
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--helper-write needs --confirm"))?;
    let candidates = enumerate(config, Some(&image))?;
    let candidate = find_candidate(&candidates, device_path)?.clone();
    if confirm != candidate.device.name {
        bail!("confirmation does not match the target device");
    }
    if let Some(reason) = &candidate.reject {
        bail!(
            "revalidation refused {}: {reason}",
            candidate.device.summary()
        );
    }
    if !candidate.device.mounted.is_empty() {
        unmount(&candidate.device)?;
    }
    validate_pair(&image, &candidate.device)?;
    write_direct(&image, &candidate.device, &mut |done, total| {
        print!("\rwritten {done}/{total} bytes");
        use std::io::Write;
        let _ = std::io::stdout().flush();
    })?;
    Ok(0)
}

fn write_direct(
    image: &ImageInfo,
    device: &Device,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<()> {
    println!(
        "raven-flash: writing {} -> {} ({})",
        image.path.display(),
        device.path.display(),
        device.model
    );
    let report = write_and_verify(image, &device.path, true, true, progress)?;
    println!();
    println!("image sha256:  {}", report.image_sha256);
    println!("target sha256: {}", report.target_sha256);
    println!("verified:      {}", report.verified);
    println!("bytes:         {}", report.bytes);
    Ok(())
}

fn elevate(options: &Options, device: &Device) -> Result<u8> {
    let exe = std::env::current_exe().context("cannot find raven-flash")?;
    let image = options
        .image
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing image"))?;
    let helper = options
        .device
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing target"))?;
    let mut args = vec![
        "--helper-write".to_string(),
        "--image".to_string(),
        image.display().to_string(),
        "--device".to_string(),
        helper.display().to_string(),
        "--confirm".to_string(),
        device.name.clone(),
    ];
    let (program, prefix) = if which("sudo") {
        ("sudo", vec!["--".to_string()])
    } else if which("pkexec") {
        ("pkexec", Vec::new())
    } else {
        bail!("write needs root; install sudo or pkexec");
    };
    let status = Command::new(program)
        .args(prefix)
        .arg(exe)
        .args(&mut args)
        .status()
        .with_context(|| format!("cannot run {program}"))?;
    Ok(u8::from(!status.success()))
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

fn is_root() -> Result<bool> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("cannot run id -u")?;
    Ok(String::from_utf8_lossy(&output.stdout).trim() == "0")
}

fn image_option(options: &Options) -> Result<Option<ImageInfo>> {
    options.image.as_deref().map(inspect).transpose()
}

fn find_candidate<'a>(candidates: &'a [Candidate], path: &Path) -> Result<&'a Candidate> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow::anyhow!("bad target path: {}", path.display()))?;
    candidates
        .iter()
        .find(|candidate| candidate.device.name == name)
        .ok_or_else(|| anyhow::anyhow!("target not found in /sys/block: {}", path.display()))
}
