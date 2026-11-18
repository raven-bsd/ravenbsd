// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `einkd` target executable: dry-run/static content before the panel exists.
//!
//! The binary uses the host-tested [`einkd`] service core with a static content source and a
//! [`DryRunPanel`]. It never opens SPI, GPIO or a network socket. The service manifest starts it
//! after login/network with `--dry-run --once`; the release image keeps it disabled by default.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use einkd::{Action, BasicFont, Clock, Config, Content, Error, Page, Panel, Service};
    use raven_abi::types::clock;
    use raven_eink::Framebuffer;
    use raven_rt::entry;

    entry!(main);

    /// Static content used before E3 supplies HTTP/TLS content.
    const DRY_PAGE: &str = "RavenBSD E-Ink\n\
                            Network: ready\n\
                            Time: scheduled\n\
                            Panel: dry-run";

    struct StaticContent {
        text: &'static str,
    }

    impl Content for StaticContent {
        fn fetch(&mut self) -> Result<Page<'_>, Error> {
            raven_rt::println!("einkd: fetch static text");
            Ok(Page::Text { text: self.text })
        }
    }

    struct DryRunPanel;

    impl Panel for DryRunPanel {
        fn refresh(&mut self, frame: &Framebuffer) -> Result<(), Error> {
            let mut black = 0usize;
            let mut y = 0usize;
            while y < frame.panel_height() {
                let mut x = 0usize;
                while x < frame.panel_width() {
                    if frame.physical_pixel(x, y) {
                        black = black.saturating_add(1);
                    }
                    x = x.saturating_add(1);
                }
                y = y.saturating_add(1);
            }
            raven_rt::println!("einkd: dry-run refresh black={black}");
            Ok(())
        }
    }

    struct WallClock;

    impl Clock for WallClock {
        fn now_secs(&self) -> u64 {
            match raven_rt::clock_get(clock::REALTIME) {
                Ok(time) => u64::try_from(time.tv_sec).unwrap_or(0),
                Err(_) => 0,
            }
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let mut once = false;
        let mut dry_run = false;
        for argument in args.iter().skip(1) {
            match argument {
                "--once" => once = true,
                "--dry-run" => dry_run = true,
                "-h" | "--help" => {
                    raven_rt::println!("usage: einkd [--dry-run] [--once]");
                    return 0;
                }
                other => {
                    raven_rt::eprintln!("einkd: unknown option {other}");
                    return 2;
                }
            }
        }
        if !dry_run {
            raven_rt::eprintln!("einkd: hardware adapter not built; use --dry-run");
            return 2;
        }

        raven_rt::println!("einkd: start mode=dry-run once={once}");
        let config = Config {
            source: "static",
            ..Config::default()
        };
        let mut service = match Service::new(
            config,
            WallClock,
            StaticContent { text: DRY_PAGE },
            DryRunPanel,
            BasicFont,
        ) {
            Ok(service) => service,
            Err(error) => {
                raven_rt::eprintln!("einkd: config: {error:?}");
                return 2;
            }
        };

        loop {
            match service.tick(true) {
                Action::Refreshed { at } => {
                    raven_rt::println!("einkd: refreshed next_at={at}");
                    if once {
                        break;
                    }
                }
                Action::Wait => {
                    if once {
                        break;
                    }
                    let _ = raven_rt::nanosleep(1_000_000_000);
                }
                Action::FetchFailed { retry_at } => {
                    raven_rt::println!("einkd: fetch failed retry_at={retry_at}");
                    if once {
                        service.shutdown();
                        return 1;
                    }
                    let _ = raven_rt::nanosleep(1_000_000_000);
                }
                Action::PanelFailed { retry_at } => {
                    raven_rt::println!("einkd: panel failed retry_at={retry_at}");
                    if once {
                        service.shutdown();
                        return 1;
                    }
                    let _ = raven_rt::nanosleep(1_000_000_000);
                }
                Action::Shutdown => break,
            }
        }
        service.shutdown();
        raven_rt::println!("einkd: done");
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
