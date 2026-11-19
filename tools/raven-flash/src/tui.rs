// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The four-screen terminal UI and its render-only test surface.
//!
//! The renderer is first-party over crossterm so the tool has no TUI-framework
//! dependency tree; the emit function and the four screen snapshots are the
//! review surface.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::device::{Candidate, FilterConfig, enumerate};
use crate::image::{ImageInfo, inspect};
use crate::writer::{WriteReport, unmount, validate_pair, write_and_verify};

/// The four TUI screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Pick the raw image.
    Image,
    /// Pick the removable target.
    Target,
    /// Confirm the summary and type the device name.
    Confirm,
    /// Write/verify progress.
    Progress,
}

/// One image in the selection list.
#[derive(Clone, Debug)]
pub struct ImageChoice {
    /// Path to the image.
    pub path: PathBuf,
    /// Size in bytes.
    pub size: u64,
}

/// TUI state (also used by the snapshot tests).
#[derive(Clone, Debug)]
pub struct App {
    /// Current screen.
    pub screen: Screen,
    /// Discovered images.
    pub images: Vec<ImageChoice>,
    /// Selected image index.
    pub image_index: usize,
    /// Candidate devices (safe and refused).
    pub devices: Vec<Candidate>,
    /// Selected device index.
    pub device_index: usize,
    /// Typed confirmation.
    pub typed: String,
    /// Status line.
    pub status: String,
    /// Write progress percent.
    pub progress: u16,
    /// Final report.
    pub report: Option<WriteReport>,
}

impl App {
    /// Build an empty app.
    pub fn new(images: Vec<ImageChoice>, devices: Vec<Candidate>) -> Self {
        Self {
            screen: Screen::Image,
            images,
            image_index: 0,
            devices,
            device_index: 0,
            typed: String::new(),
            status: "Select a raw image. Up/Down move, Enter selects, q quits.".to_string(),
            progress: 0,
            report: None,
        }
    }

    /// The selected image, if any.
    pub fn image(&self) -> Option<&ImageChoice> {
        self.images.get(self.image_index)
    }

    /// The selected device, if any.
    pub fn device(&self) -> Option<&Candidate> {
        self.devices.get(self.device_index)
    }
}

/// Render the current screen to text (the snapshot and TUI output).
pub fn render(app: &App) -> String {
    let mut out = String::new();
    match app.screen {
        Screen::Image => {
            out.push_str("=== raven-flash: select image ===\n");
            for (index, image) in app.images.iter().enumerate() {
                out.push_str(&format!(
                    "{} {} ({} bytes)\n",
                    if index == app.image_index { ">" } else { " " },
                    image.path.display(),
                    image.size
                ));
            }
            out.push_str("\nUp/Down  Enter  q\n");
        }
        Screen::Target => {
            out.push_str("=== raven-flash: select removable target ===\n");
            for (index, candidate) in app.devices.iter().enumerate() {
                let marker = if index == app.device_index { ">" } else { " " };
                match &candidate.reject {
                    Some(reason) => out.push_str(&format!(
                        "{} {} [refused: {reason}]\n",
                        marker,
                        candidate.device.summary()
                    )),
                    None => out.push_str(&format!("{} {}\n", marker, candidate.device.summary())),
                }
            }
            out.push_str("\nUp/Down  Enter  Esc  q\n");
        }
        Screen::Confirm => {
            let image = app
                .image()
                .map(|image| format!("{} ({} bytes)", image.path.display(), image.size))
                .unwrap_or_else(|| "none".to_string());
            let device = app
                .device()
                .map(|candidate| candidate.device.summary())
                .unwrap_or_else(|| "none".to_string());
            let mounted = app
                .device()
                .map(|candidate| {
                    if candidate.device.mounted.is_empty() {
                        "none".to_string()
                    } else {
                        candidate
                            .device
                            .mounted
                            .iter()
                            .map(|mount| mount.target.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                })
                .unwrap_or_else(|| "none".to_string());
            out.push_str("=== confirm ===\n");
            out.push_str("RAW IMAGE WRITE\n");
            out.push_str(&format!("image: {image}\n"));
            out.push_str(&format!("target: {device}\n"));
            out.push_str(&format!("mounted partitions to unmount: {mounted}\n"));
            out.push_str("Type the target device name to confirm.\n");
            out.push_str(&format!("{}\n", app.status));
            out.push_str(&format!("\n> {}\n", app.typed));
            out.push_str("\nEnter writes  Esc back  q quits\n");
        }
        Screen::Progress => {
            out.push_str(&format!("=== writing: {}% ===\n", app.progress));
            out.push_str(&format!("{}\n", app.status));
            if let Some(report) = &app.report {
                out.push_str(&format!("image sha256: {}\n", report.image_sha256));
                out.push_str(&format!("target sha256: {}\n", report.target_sha256));
                out.push_str(&format!("verified: {}\n", report.verified));
                out.push_str(&format!("bytes: {}\n", report.bytes));
            }
        }
    }
    out
}

fn draw(app: &App) -> Result<()> {
    let text = render(app);
    let mut stdout = std::io::stdout();
    execute!(stdout, crossterm::cursor::MoveTo(0, 0))?;
    execute!(
        stdout,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
    )?;
    for line in text.lines() {
        writeln!(stdout, "{line}")?;
    }
    stdout.flush()?;
    Ok(())
}

/// Discover `.img` files under `build/` and the current directory (depth two).
pub fn discover_images(root: &Path) -> Vec<ImageChoice> {
    let mut images = Vec::new();
    for base in [root.join("build"), root.to_path_buf()] {
        collect_images(&base, 0, &mut images);
    }
    images.sort_by(|left, right| left.path.cmp(&right.path));
    images.dedup_by(|left, right| left.path == right.path);
    images
}

fn collect_images(dir: &Path, depth: usize, out: &mut Vec<ImageChoice>) {
    if depth > 2 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_images(&path, depth + 1, out);
        } else if path.extension().and_then(|value| value.to_str()) == Some("img")
            && let Ok(metadata) = entry.metadata()
        {
            out.push(ImageChoice {
                path,
                size: metadata.len(),
            });
        }
    }
}

/// Run the interactive TUI.
pub fn run(config: FilterConfig) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("the TUI needs a terminal; use --list or --dry-run");
    }
    let images = discover_images(Path::new("."));
    let devices = enumerate(&config, None)?;
    let mut app = App::new(images, devices);

    enable_raw_mode().context("cannot enable raw mode")?;
    execute!(std::io::stdout(), EnterAlternateScreen)
        .context("cannot enter the alternate screen")?;
    let result = event_loop(&mut app);
    disable_raw_mode().ok();
    execute!(std::io::stdout(), LeaveAlternateScreen).ok();
    result
}

fn event_loop(app: &mut App) -> Result<()> {
    let mut receiver: Option<Receiver<WriteEvent>> = None;
    loop {
        let mut finished = false;
        if let Some(rx) = receiver.as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(WriteEvent::Progress(done, total)) => {
                        app.progress = done
                            .saturating_mul(100)
                            .checked_div(total)
                            .unwrap_or(0)
                            .min(100) as u16;
                        app.status = format!("written {done}/{total} bytes");
                    }
                    Ok(WriteEvent::Done(result)) => {
                        match result {
                            Ok(report) => {
                                app.progress = 100;
                                app.status = "verified: readback matches the image".to_string();
                                app.report = Some(report);
                            }
                            Err(error) => {
                                app.status = format!("write failed: {error}");
                            }
                        }
                        finished = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
        }
        if finished {
            receiver = None;
        }
        draw(app)?;
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char(value) if app.screen == Screen::Confirm => app.typed.push(value),
            KeyCode::Char('q') => return Ok(()),
            KeyCode::Esc => match app.screen {
                Screen::Target => app.screen = Screen::Image,
                Screen::Confirm => app.screen = Screen::Target,
                _ => {}
            },
            KeyCode::Down | KeyCode::Char('j') => match app.screen {
                Screen::Image if !app.images.is_empty() => {
                    app.image_index = (app.image_index + 1).min(app.images.len() - 1);
                }
                Screen::Target if !app.devices.is_empty() => {
                    app.device_index = (app.device_index + 1).min(app.devices.len() - 1);
                }
                _ => {}
            },
            KeyCode::Up | KeyCode::Char('k') => match app.screen {
                Screen::Image => app.image_index = app.image_index.saturating_sub(1),
                Screen::Target => app.device_index = app.device_index.saturating_sub(1),
                _ => {}
            },
            KeyCode::Enter => match app.screen {
                Screen::Image => {
                    if app.image().is_some() {
                        app.screen = Screen::Target;
                        app.status = "Select a removable target.".to_string();
                    }
                }
                Screen::Target => {
                    let device = app.device().cloned();
                    match device {
                        Some(candidate) if candidate.reject.is_none() => {
                            app.screen = Screen::Confirm;
                            app.typed.clear();
                            app.status = "Review the summary and type the device name.".to_string();
                        }
                        Some(candidate) => {
                            app.status = format!(
                                "refused: {}",
                                candidate
                                    .reject
                                    .unwrap_or_else(|| "unsafe target".to_string())
                            );
                        }
                        None => app.status = "no target selected".to_string(),
                    }
                }
                Screen::Confirm => {
                    let Some(image) = app.image().cloned() else {
                        app.status = "no image selected".to_string();
                        continue;
                    };
                    let Some(candidate) = app.device().cloned() else {
                        app.status = "no target selected".to_string();
                        continue;
                    };
                    if app.typed != candidate.device.name {
                        app.status = format!("type `{}` to confirm", candidate.device.name);
                        continue;
                    }
                    match inspect(&image.path) {
                        Ok(info) => {
                            if let Err(error) = validate_pair(&info, &candidate.device) {
                                app.status = format!("refused: {error}");
                                continue;
                            }
                            if !candidate.device.mounted.is_empty()
                                && let Err(error) = unmount(&candidate.device)
                            {
                                app.status = format!("unmount failed: {error}");
                                continue;
                            }
                            app.screen = Screen::Progress;
                            app.progress = 0;
                            app.status = "writing...".to_string();
                            receiver = Some(start_write(info, candidate.device.path.clone()));
                        }
                        Err(error) => app.status = format!("image error: {error}"),
                    }
                }
                Screen::Progress => {}
            },
            KeyCode::Backspace if app.screen == Screen::Confirm => {
                app.typed.pop();
            }
            KeyCode::Char(_) => {}
            _ => {}
        }
    }
}

enum WriteEvent {
    Progress(u64, u64),
    Done(std::result::Result<WriteReport, String>),
}

fn start_write(image: ImageInfo, target: PathBuf) -> Receiver<WriteEvent> {
    let (sender, receiver) = channel();
    let progress = sender.clone();
    std::thread::spawn(move || {
        let result = write_and_verify(&image, &target, true, true, &mut |done, total| {
            let _ = progress.send(WriteEvent::Progress(done, total));
        })
        .map_err(|error| format!("{error:#}"));
        let _ = sender.send(WriteEvent::Done(result));
    });
    receiver
}

#[cfg(test)]
#[path = "tui_ut.rs"]
mod tests;
