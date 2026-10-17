use crate::{
    content::Exercise,
    engine::{FinishReason, Mode, Report},
};
use std::time::{SystemTime, UNIX_EPOCH};
pub type HistoryResult<T> = Result<T, Box<dyn std::error::Error>>;
pub const METRIC_VERSION: i64 = 1;
pub const DAY_MS: i64 = 86_400_000;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy)]
pub struct Counts {
    pub attempts: u64,
    pub correct_attempts: u64,
    pub current_correct: u64,
    pub backspaces: u64,
    pub uncorrected: u64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub id: String,
    pub completed_ms: i64,
    pub mode: Mode,
    pub seconds: u64,
    pub elapsed_ns: i64,
    pub reason: FinishReason,
    pub exercise: Exercise,
    pub attempts: u64,
    pub correct_attempts: u64,
    pub current_correct: u64,
    pub backspaces: u64,
    pub uncorrected: u64,
    pub wpm: Option<f64>,
    pub raw_wpm: Option<f64>,
    pub accuracy: Option<f64>,
    pub metric_version: i64,
    pub app_version: String,
}
impl Record {
    pub fn new(
        id: String,
        completed_ms: i64,
        mode: Mode,
        seconds: u64,
        elapsed_ns: i64,
        c: Counts,
    ) -> Self {
        let minutes = elapsed_ns as f64 / 60_000_000_000.0;
        let measurable = minutes > 0.0 && c.attempts > 0;
        Self {
            id,
            completed_ms,
            mode,
            seconds,
            elapsed_ns,
            reason: if mode == Mode::Words {
                FinishReason::Timeout
            } else {
                FinishReason::SnippetExhausted
            },
            exercise: if mode == Mode::Words {
                Exercise::words()
            } else {
                Exercise::fixture()
            },
            attempts: c.attempts,
            correct_attempts: c.correct_attempts,
            current_correct: c.current_correct,
            backspaces: c.backspaces,
            uncorrected: c.uncorrected,
            wpm: measurable.then(|| c.current_correct as f64 / 5.0 / minutes),
            raw_wpm: measurable.then(|| c.attempts as f64 / 5.0 / minutes),
            accuracy: measurable.then(|| c.correct_attempts as f64 * 100.0 / c.attempts as f64),
            metric_version: METRIC_VERSION,
            app_version: env!("CARGO_PKG_VERSION").into(),
        }
    }
    pub fn from_report(
        id: String,
        completed_ms: i64,
        r: &Report,
        exercise: Exercise,
    ) -> HistoryResult<Self> {
        let mut record = Self::new(
            id,
            completed_ms,
            r.settings.mode,
            r.settings.duration.seconds(),
            i64::try_from(r.elapsed.as_nanos())?,
            Counts {
                attempts: r.counters.attempts,
                correct_attempts: r.counters.correct_attempts,
                current_correct: r.counters.current_correct as u64,
                backspaces: r.counters.backspaces,
                uncorrected: r.uncorrected as u64,
            },
        );
        record.reason = r.reason;
        record.exercise = exercise;
        record.wpm = r.metrics.wpm;
        record.raw_wpm = r.metrics.raw_wpm;
        record.accuracy = r.metrics.accuracy;
        record.validate()?;
        Ok(record)
    }
    pub fn validate(&self) -> HistoryResult<()> {
        if uuid::Uuid::try_parse(&self.id).is_err()
            || self.id.len() != 36
            || self.completed_ms < 0
            || ![30, 60, 120].contains(&self.seconds)
            || self.elapsed_ns < 0
            || self.elapsed_ns as u64 > self.seconds * 1_000_000_000
            || self.attempts == 0
            || self.correct_attempts > self.attempts
            || self.current_correct > self.correct_attempts
            || self
                .current_correct
                .checked_add(self.uncorrected)
                .is_none_or(|n| n > self.attempts)
            || self.uncorrected > self.attempts - self.correct_attempts
            || self.backspaces > self.attempts
            || [
                self.attempts,
                self.correct_attempts,
                self.current_correct,
                self.backspaces,
                self.uncorrected,
            ]
            .iter()
            .any(|n| *n > i64::MAX as u64)
            || self.metric_version < 1
            || self.exercise.version < 1
            || self.exercise.pool_version < 1
            || (self.mode == Mode::Words && self.reason != FinishReason::Timeout)
        {
            return Err("invalid history summary".into());
        }
        for s in [&self.exercise.id, &self.exercise.pool, &self.app_version] {
            if s.is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
                return Err("invalid history metadata".into());
            }
        }
        if self
            .exercise
            .language
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 40 || s.chars().any(char::is_control))
        {
            return Err("invalid language".into());
        }
        if [self.wpm, self.raw_wpm, self.accuracy]
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || *v < 0.0)
            || self.accuracy.is_some_and(|v| v > 100.0)
        {
            return Err("invalid history metrics".into());
        }
        if self.metric_version == METRIC_VERSION {
            let expected = Self::new(
                self.id.clone(),
                self.completed_ms,
                self.mode,
                self.seconds,
                self.elapsed_ns,
                Counts {
                    attempts: self.attempts,
                    correct_attempts: self.correct_attempts,
                    current_correct: self.current_correct,
                    backspaces: self.backspaces,
                    uncorrected: self.uncorrected,
                },
            );
            for (actual, want) in [
                (self.wpm, expected.wpm),
                (self.raw_wpm, expected.raw_wpm),
                (self.accuracy, expected.accuracy),
            ] {
                match (actual, want) {
                    (None, None) => {}
                    (Some(a), Some(b)) if (a - b).abs() <= 1e-8 * b.abs().max(1.0) => {}
                    _ => return Err("inconsistent history metrics".into()),
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub mode: Option<Mode>,
    pub seconds: Option<u64>,
    pub exercise: Option<String>,
    pub exercise_version: Option<i64>,
    pub language: Option<String>,
    pub reason: Option<FinishReason>,
    pub metric_version: i64,
    pub pool_version: i64,
    pub days: Option<i64>,
    pub now_ms: i64,
}
impl Default for Filter {
    fn default() -> Self {
        Self {
            mode: None,
            seconds: None,
            exercise: None,
            exercise_version: None,
            language: None,
            reason: None,
            metric_version: 1,
            pool_version: 1,
            days: None,
            now_ms: now_ms(),
        }
    }
}
impl Filter {
    pub fn validate(&self) -> HistoryResult<()> {
        if self.seconds.is_some_and(|v| ![30, 60, 120].contains(&v))
            || self.days.is_some_and(|v| ![7, 30].contains(&v))
            || self.metric_version < 1
            || self.pool_version < 1
            || self.now_ms < 0
            || self.exercise_version.is_some_and(|v| v < 1)
        {
            return Err("invalid history filter".into());
        }
        Ok(())
    }
    pub fn comparable(&self) -> bool {
        self.seconds.is_some()
            && (self.mode == Some(Mode::Words)
                || (self.mode == Some(Mode::Code)
                    && self.exercise.is_some()
                    && self.exercise_version.is_some()
                    && self.reason.is_some()))
    }
    pub fn since(&self) -> i64 {
        self.days.map_or(0, |d| {
            (self.now_ms.div_euclid(DAY_MS) - (d - 1))
                .max(0)
                .saturating_mul(DAY_MS)
        })
    }
}
pub type Cursor = (i64, String);
#[derive(Debug, Clone, Default)]
pub struct Page {
    pub rows: Vec<Record>,
    pub next: Option<Cursor>,
}
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub count: u64,
    pub practice_ns: f64,
    pub best: Option<f64>,
    pub median: Option<f64>,
    pub accuracy: Option<f64>,
    pub change: Option<f64>,
    pub trend: Vec<(f64, f64, f64)>,
    pub daily: Vec<f64>,
}
pub fn date_label(ms: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .map(|d| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}Z",
                d.year(),
                u8::from(d.month()),
                d.day(),
                d.hour(),
                d.minute()
            )
        })
        .unwrap_or_else(|_| "Unavailable".into())
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SaveStatus {
    #[default]
    Disabled,
    Pending,
    Saved,
    Unsaved,
    Cleared,
}
impl SaveStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "History disabled",
            Self::Pending => "Saving locally...",
            Self::Saved => "Saved locally",
            Self::Unsaved => "Unsaved; r retries saving",
            Self::Cleared => "History cleared",
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HistoryTab {
    #[default]
    Recent,
    Progress,
}
#[derive(Debug, Default)]
pub struct HistoryModel {
    pub enabled: bool,
    pub save: SaveStatus,
    pub tab: HistoryTab,
    pub filter: Filter,
    pub page: Page,
    pub stats: Stats,
    pub cursors: Vec<Option<Cursor>>,
    pub scroll: usize,
    pub graph: usize,
    pub scope: usize,
    pub reload: bool,
    pub retry: bool,
    pub confirm_clear: bool,
    pub clear_requested: bool,
    pub unavailable: bool,
}
impl HistoryModel {
    pub fn reset_page(&mut self) {
        self.cursors = vec![None];
        self.scroll = 0;
        self.reload = true;
    }
}
