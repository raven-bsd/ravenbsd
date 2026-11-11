// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scripted serial dialogue: wait for output, then write to the guest's serial input.
//!
//! A case's `[[step]]` tables run in order while the process is alive. A `wait` regex is
//! matched against the complete serial lines and against the current partial line, so a prompt
//! that is not newline-terminated (`raven# `) can be detected. A line (or the consumed part of a
//! partial line) is consumed by a match and is not offered to a later wait.
//!
//! The engine is deliberately free of I/O: the runner feeds it lines and partial lines and asks
//! for the bytes that are due. That keeps the state machine unit-testable without a process.

use std::time::{Duration, Instant};

use regex::Regex;

use crate::case::{Case, DEFAULT_STEP_DELAY_MS};

/// One compiled `[[step]]`.
struct PlanStep {
    wait: Option<Regex>,
    /// Wait pattern, or the send text, used in failure messages.
    pattern: String,
    send: Option<Vec<u8>>,
    delay: Duration,
    timeout: Duration,
}

/// What to write to the guest's serial input after a step matched.
struct Scheduled {
    at: Instant,
    bytes: Vec<u8>,
}

/// The ordered script, the unconsumed serial lines and the pending writes.
pub(crate) struct Dialogue {
    steps: Vec<PlanStep>,
    current: usize,
    lines: Vec<String>,
    partial: String,
    partial_consumed: bool,
    scheduled: Option<Scheduled>,
    /// When the current step became active in the stream order.
    started: Instant,
}

impl Dialogue {
    /// Build the dialogue for a case, or `None` when it declares no steps.
    pub(crate) fn from_case(case: &Case) -> Option<Self> {
        if case.steps.is_empty() {
            return None;
        }
        let mut steps = Vec::with_capacity(case.steps.len());
        for (step, regex) in case.steps.iter().zip(case.step_regexes()) {
            let pattern = match (regex, &step.send) {
                (Some(regex), _) => regex.as_str().to_string(),
                (None, Some(send)) => format!("send {send:?}"),
                // The parser rejects steps with neither field, so this is only a fallback.
                (None, None) => "empty step".to_string(),
            };
            steps.push(PlanStep {
                wait: regex.clone(),
                pattern,
                send: step.send.as_ref().map(|text| text.as_bytes().to_vec()),
                delay: Duration::from_millis(step.delay_ms.unwrap_or(DEFAULT_STEP_DELAY_MS)),
                timeout: Duration::from_secs(step.timeout_secs.unwrap_or(case.spec.timeout_secs)),
            });
        }
        Some(Self {
            steps,
            current: 0,
            lines: Vec::new(),
            partial: String::new(),
            partial_consumed: false,
            scheduled: None,
            started: Instant::now(),
        })
    }

    /// Whether every step has matched and every send has been queued.
    pub(crate) fn is_complete(&self) -> bool {
        self.current >= self.steps.len()
    }

    /// The step still waiting for input: its index, its pattern and its timeout.
    pub(crate) fn pending_step(&self) -> Option<(usize, &str, Duration)> {
        let step = self.steps.get(self.current)?;
        Some((self.current, step.pattern.as_str(), step.timeout))
    }

    /// Feed one complete serial line (cleaned, without the newline).
    pub(crate) fn line(&mut self, text: &str, now: Instant) {
        self.partial.clear();
        if self.partial_consumed {
            // The line was already consumed while it was a partial prompt; drop the rest.
            self.partial_consumed = false;
        } else {
            self.lines.push(text.to_string());
        }
        self.advance(now);
    }

    /// Feed the current partial line, which grows until a newline arrives.
    pub(crate) fn partial(&mut self, text: &str, now: Instant) {
        if self.partial_consumed {
            return;
        }
        self.partial.clear();
        self.partial.push_str(text);
        self.advance(now);
    }

    /// Take the bytes that are due to be written, if the send delay has elapsed.
    pub(crate) fn poll_send(&mut self, now: Instant) -> Option<Vec<u8>> {
        // Send-only steps can be due before any serial input arrives.
        self.advance(now);
        let scheduled = self.scheduled.as_ref()?;
        if now < scheduled.at {
            return None;
        }
        let bytes = self.scheduled.take()?.bytes;
        self.current += 1;
        self.started = now;
        self.advance(now);
        Some(bytes)
    }

    /// Deadline of the current `wait`, or `None` when no step is waiting.
    pub(crate) fn wait_deadline(&self) -> Option<Instant> {
        let step = self.steps.get(self.current)?;
        if step.wait.is_none() || self.scheduled.is_some() {
            return None;
        }
        Some(self.started + step.timeout)
    }

    fn advance(&mut self, now: Instant) {
        loop {
            if self.scheduled.is_some() {
                return;
            }
            let Some(step) = self.steps.get(self.current) else {
                self.lines.clear();
                return;
            };
            if let Some(regex) = &step.wait {
                if let Some(index) = self.lines.iter().position(|line| regex.is_match(line)) {
                    self.lines.drain(..=index);
                } else if !self.partial_consumed && regex.is_match(&self.partial) {
                    // A partial prompt is the newest thing on the stream, so anything still
                    // buffered before it is skipped just like a consumed complete line.
                    self.lines.clear();
                    self.partial.clear();
                    self.partial_consumed = true;
                } else {
                    return;
                }
            }
            let delay = step.delay;
            let send = step.send.clone();
            if let Some(bytes) = send {
                // Keep the step current until the delayed write is actually performed, so an
                // early process exit is reported against this step.
                self.scheduled = Some(Scheduled {
                    at: now + delay,
                    bytes,
                });
                return;
            }
            self.current += 1;
            self.started = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case::Case;

    fn dialogue(step_tables: &str, timeout: u64) -> Dialogue {
        let text = format!(
            "name = \"d\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = {timeout}\n\
             expect_exit = \"success\"\n{step_tables}"
        );
        let case = Case::parse(&text).expect("case");
        Dialogue::from_case(&case).expect("dialogue")
    }

    fn send(dialogue: &mut Dialogue, now: Instant) -> Option<String> {
        dialogue
            .poll_send(now)
            .map(|bytes| String::from_utf8(bytes).expect("UTF-8 payload"))
    }

    #[test]
    fn waits_match_complete_lines_in_order() {
        let mut dialogue = dialogue(
            "[[step]]\nwait = \"^one$\"\n[[step]]\nwait = \"^two$\"\n",
            5,
        );
        let now = Instant::now();
        dialogue.line("one", now);
        assert_eq!(dialogue.pending_step().unwrap().0, 1);
        dialogue.line("two", now);
        assert!(dialogue.is_complete());
    }

    #[test]
    fn a_wait_does_not_reuse_an_earlier_line() {
        let mut dialogue = dialogue(
            "[[step]]\nwait = \"^same$\"\n[[step]]\nwait = \"^same$\"\n",
            5,
        );
        let now = Instant::now();
        dialogue.line("same", now);
        assert_eq!(dialogue.pending_step().unwrap().0, 1);
        assert!(!dialogue.is_complete());
        dialogue.line("same", now);
        assert!(dialogue.is_complete());
    }

    #[test]
    fn a_partial_prompt_matches_without_a_newline() {
        let mut dialogue = dialogue("[[step]]\nwait = \"^ready> $\"\n", 5);
        let now = Instant::now();
        dialogue.partial("ready> ", now);
        assert!(dialogue.is_complete());
    }

    #[test]
    fn a_partial_match_consumes_the_rest_of_the_line() {
        let mut dialogue = dialogue(
            "[[step]]\nwait = \"ready\"\n[[step]]\nwait = \"typed\"\n",
            5,
        );
        let now = Instant::now();
        dialogue.partial("ready> ", now);
        assert_eq!(dialogue.pending_step().unwrap().0, 1);
        dialogue.partial("ready> typed", now);
        assert_eq!(dialogue.pending_step().unwrap().0, 1);
        dialogue.line("ready> typed", now);
        assert!(!dialogue.is_complete());
    }

    #[test]
    fn partial_bytes_after_a_match_are_ignored() {
        let mut dialogue = dialogue("[[step]]\nwait = \"go\"\n[[step]]\nwait = \"later\"\n", 5);
        let now = Instant::now();
        dialogue.partial("go", now);
        dialogue.partial("go now", now);
        dialogue.line("go now", now);
        assert_eq!(dialogue.pending_step().unwrap().0, 1);
    }

    #[test]
    fn a_send_is_scheduled_after_its_delay() {
        let mut dialogue = dialogue(
            "[[step]]\nwait = \"go\"\nsend = \"hello\\n\"\ndelay_ms = 50\n",
            5,
        );
        let now = Instant::now();
        dialogue.partial("go", now);
        assert_eq!(send(&mut dialogue, now + Duration::from_millis(10)), None);
        assert_eq!(
            send(&mut dialogue, now + Duration::from_millis(60)),
            Some("hello\n".to_string())
        );
        assert!(dialogue.is_complete());
    }

    #[test]
    fn send_only_steps_fire_without_any_input() {
        let mut dialogue = dialogue("[[step]]\nsend = \"hi\\n\"\ndelay_ms = 0\n", 5);
        let now = Instant::now();
        assert_eq!(send(&mut dialogue, now), Some("hi\n".to_string()));
        assert!(dialogue.is_complete());
    }

    #[test]
    fn chained_sends_preserve_order_and_delay() {
        let mut dialogue = dialogue(
            "[[step]]\nsend = \"a\\n\"\ndelay_ms = 10\n\
             [[step]]\nsend = \"b\\n\"\ndelay_ms = 10\n",
            5,
        );
        let now = Instant::now();
        assert_eq!(send(&mut dialogue, now), None);
        assert_eq!(
            send(&mut dialogue, now + Duration::from_millis(20)),
            Some("a\n".to_string())
        );
        assert_eq!(
            send(&mut dialogue, now + Duration::from_millis(25)),
            None,
            "the second send keeps its own delay"
        );
        assert_eq!(
            send(&mut dialogue, now + Duration::from_millis(40)),
            Some("b\n".to_string())
        );
        assert!(dialogue.is_complete());
    }

    #[test]
    fn a_wait_skips_lines_before_its_match() {
        let mut dialogue = dialogue("[[step]]\nwait = \"^target$\"\n", 5);
        let now = Instant::now();
        dialogue.line("noise", now);
        dialogue.line("target", now);
        assert!(dialogue.is_complete());
    }

    #[test]
    fn the_wait_deadline_is_the_step_timeout() {
        let dialogue = dialogue("[[step]]\nwait = \"never\"\ntimeout_secs = 2\n", 10);
        let start = dialogue.started;
        assert_eq!(
            dialogue.wait_deadline(),
            Some(start + Duration::from_secs(2))
        );
        assert_eq!(dialogue.pending_step().unwrap().2, Duration::from_secs(2));
    }

    #[test]
    fn a_complete_dialogue_has_no_pending_step_or_deadline() {
        let mut dialogue = dialogue("[[step]]\nwait = \"x\"\n", 5);
        let now = Instant::now();
        dialogue.line("x", now);
        assert!(dialogue.is_complete());
        assert!(dialogue.pending_step().is_none());
        assert!(dialogue.wait_deadline().is_none());
    }

    #[test]
    fn a_case_without_steps_has_no_dialogue() {
        let text = "name = \"d\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
                    expect_exit = \"success\"\n";
        let case = Case::parse(text).expect("case");
        assert!(Dialogue::from_case(&case).is_none());
    }
}
