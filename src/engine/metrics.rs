use std::time::Duration;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counters {
    pub attempts: u64,
    pub correct_attempts: u64,
    pub backspaces: u64,
    pub current_correct: usize,
}

impl Counters {
    pub fn errors(self) -> u64 {
        self.attempts - self.correct_attempts
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub accuracy: Option<f64>,
    pub raw_wpm: Option<f64>,
    pub wpm: Option<f64>,
}

impl Metrics {
    pub fn calculate(counters: Counters, elapsed: Option<Duration>) -> Self {
        let minutes = elapsed.map_or(0.0, |d| d.as_secs_f64() / 60.0);
        if counters.attempts == 0 || minutes == 0.0 {
            return Self {
                accuracy: None,
                raw_wpm: None,
                wpm: None,
            };
        }
        Self {
            accuracy: Some(counters.correct_attempts as f64 * 100.0 / counters.attempts as f64),
            raw_wpm: Some(counters.attempts as f64 / 5.0 / minutes),
            wpm: Some(counters.current_correct as f64 / 5.0 / minutes),
        }
    }
}
