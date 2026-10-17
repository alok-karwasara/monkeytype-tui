pub mod metrics;
pub mod prompt;

use metrics::{Counters, Metrics};
use prompt::{Prompt, PromptError};
use std::{fmt, time::Duration};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Words,
    Code,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Words => "words",
            Self::Code => "code",
        })
    }
}

impl Mode {
    pub fn other(self) -> Self {
        match self {
            Self::Words => Self::Code,
            Self::Code => Self::Words,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TestDuration {
    Seconds30,
    #[default]
    Seconds60,
    Seconds120,
}

impl TestDuration {
    pub fn seconds(self) -> u64 {
        match self {
            Self::Seconds30 => 30,
            Self::Seconds60 => 60,
            Self::Seconds120 => 120,
        }
    }
    pub fn as_duration(self) -> Duration {
        Duration::from_secs(self.seconds())
    }
    pub fn from_seconds(seconds: u64) -> Option<Self> {
        match seconds {
            30 => Some(Self::Seconds30),
            60 => Some(Self::Seconds60),
            120 => Some(Self::Seconds120),
            _ => None,
        }
    }
    pub fn cycle(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::Seconds30, true) | (Self::Seconds120, false) => Self::Seconds60,
            (Self::Seconds60, true) | (Self::Seconds30, false) => Self::Seconds120,
            (Self::Seconds120, true) | (Self::Seconds60, false) => Self::Seconds30,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub mode: Mode,
    pub duration: TestDuration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    Timeout,
    SnippetExhausted,
}

impl fmt::Display for FinishReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Timeout => "time limit",
            Self::SnippetExhausted => "snippet complete",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Running {
        started_at: Duration,
    },
    Finished {
        elapsed: Duration,
        reason: FinishReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineInput {
    Character(char),
    Backspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOutcome {
    Accepted,
    Ignored,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub settings: Settings,
    pub elapsed: Duration,
    pub reason: FinishReason,
    pub counters: Counters,
    pub uncorrected: usize,
    pub metrics: Metrics,
}

#[derive(Debug)]
pub struct Session {
    prompt: Prompt,
    settings: Settings,
    phase: Phase,
    typed: Vec<char>,
    counters: Counters,
}

impl Session {
    pub fn new(prompt: Prompt, duration: TestDuration) -> Self {
        let settings = Settings {
            mode: prompt.mode(),
            duration,
        };
        Self {
            prompt,
            settings,
            phase: Phase::Ready,
            typed: Vec::new(),
            counters: Counters::default(),
        }
    }
    pub fn prompt(&self) -> &Prompt {
        &self.prompt
    }
    pub fn settings(&self) -> Settings {
        self.settings
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn typed(&self) -> &[char] {
        &self.typed
    }
    pub fn cursor(&self) -> usize {
        self.typed.len()
    }
    pub fn counters(&self) -> Counters {
        self.counters
    }
    pub fn uncorrected(&self) -> usize {
        self.cursor() - self.counters.current_correct
    }
    pub fn elapsed(&self, now: Duration) -> Option<Duration> {
        match self.phase {
            Phase::Ready => None,
            Phase::Running { started_at } => Some(
                now.saturating_sub(started_at)
                    .min(self.settings.duration.as_duration()),
            ),
            Phase::Finished { elapsed, .. } => Some(elapsed),
        }
    }
    pub fn remaining(&self, now: Duration) -> Duration {
        self.settings
            .duration
            .as_duration()
            .saturating_sub(self.elapsed(now).unwrap_or_default())
    }
    pub fn metrics(&self, now: Duration) -> Metrics {
        Metrics::calculate(self.counters, self.elapsed(now))
    }

    pub fn advance_time(&mut self, now: Duration) {
        if let Phase::Running { started_at } = self.phase
            && now.saturating_sub(started_at) >= self.settings.duration.as_duration()
        {
            self.phase = Phase::Finished {
                elapsed: self.settings.duration.as_duration(),
                reason: FinishReason::Timeout,
            };
        }
    }

    pub fn apply(&mut self, input: EngineInput, now: Duration) -> InputOutcome {
        self.advance_time(now);
        if matches!(self.phase, Phase::Finished { .. }) {
            return InputOutcome::Ignored;
        }
        match input {
            EngineInput::Character(c) => {
                if !c.is_ascii() {
                    return InputOutcome::Unsupported;
                }
                if !(c.is_ascii_graphic() || c == ' ') {
                    return InputOutcome::Ignored;
                }
                let Some(expected) = self.prompt.expected().get(self.cursor()) else {
                    return InputOutcome::Ignored;
                };
                if self.phase == Phase::Ready {
                    self.phase = Phase::Running { started_at: now };
                }
                self.counters.attempts += 1;
                if c == expected.character {
                    self.counters.correct_attempts += 1;
                    self.counters.current_correct += 1;
                }
                self.typed.push(c);
                if self.settings.mode == Mode::Code && self.cursor() == self.prompt.expected().len()
                {
                    self.phase = Phase::Finished {
                        elapsed: self.elapsed(now).unwrap_or_default(),
                        reason: FinishReason::SnippetExhausted,
                    };
                }
                InputOutcome::Accepted
            }
            EngineInput::Backspace => {
                if let Some(c) = self.typed.pop() {
                    self.counters.backspaces += 1;
                    if self
                        .prompt
                        .expected()
                        .get(self.cursor())
                        .is_some_and(|e| e.character == c)
                    {
                        self.counters.current_correct -= 1;
                    }
                    InputOutcome::Accepted
                } else {
                    InputOutcome::Ignored
                }
            }
        }
    }

    pub fn append_words(&mut self, words: &str) -> Result<(), PromptError> {
        self.prompt.append_words(words)
    }

    pub fn report(&self) -> Option<Report> {
        match self.phase {
            Phase::Finished { elapsed, reason } => Some(Report {
                settings: self.settings,
                elapsed,
                reason,
                counters: self.counters,
                uncorrected: self.uncorrected(),
                metrics: Metrics::calculate(self.counters, Some(elapsed)),
            }),
            _ => None,
        }
    }
}
