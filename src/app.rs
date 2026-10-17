#[cfg(feature = "test-support")]
use crate::engine::prompt::Prompt;
use crate::{
    content::{Exercise, Prepared, Selector},
    engine::{
        EngineInput, InputOutcome, Mode, Phase, Report, Session, Settings, prompt::PromptError,
    },
    history::{HistoryModel, HistoryTab, SaveStatus},
    input::Action,
};
use std::time::Duration;

pub const MIN_WIDTH: u16 = 40;
pub const MIN_HEIGHT: u16 = 12;

#[derive(Debug)]
pub enum Screen {
    Practice(Session),
    Settings {
        draft: Settings,
        field: usize,
        original: Box<Session>,
    },
    Results(Report),
    History {
        previous: Box<Screen>,
    },
    Transition,
}

#[derive(Debug, Clone)]
pub struct Completion {
    pub id: String,
    pub report: Report,
    pub exercise: Exercise,
}

#[derive(Debug)]
pub struct App {
    pub screen: Screen,
    pub settings: Settings,
    pub abandon: bool,
    pub notice: Option<&'static str>,
    pub quit: bool,
    width: u16,
    height: u16,
    pub history: HistoryModel,
    pub exercise: Exercise,
    pub selector: Selector,
    pub pending_code: bool,
    session_id: String,
    completion: Option<Completion>,
    #[cfg(feature = "test-support")]
    fixture: Option<(String, Mode)>,
}

impl App {
    pub fn new(settings: Settings) -> Result<Self, Box<dyn std::error::Error>> {
        let mut selector = Selector::try_new()?;
        let prepared = match settings.mode {
            Mode::Words => Prepared {
                meta: Exercise::words(),
                source: selector.words(),
            },
            Mode::Code => selector.code(),
        };
        let prompt = crate::engine::prompt::Prompt::new(&prepared.source, settings.mode)?;
        let session_id = selector.session_id();
        Ok(Self {
            screen: Screen::Practice(Session::new(prompt, settings.duration)),
            settings,
            abandon: false,
            notice: None,
            quit: false,
            width: 80,
            height: 24,
            history: HistoryModel::default(),
            exercise: prepared.meta,
            selector,
            pending_code: settings.mode == Mode::Code,
            session_id,
            completion: None,
            #[cfg(feature = "test-support")]
            fixture: None,
        })
    }

    #[cfg(feature = "test-support")]
    pub fn with_fixture(
        settings: Settings,
        source: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let prompt = Prompt::new(source, settings.mode)?;
        let mut app = Self::new(settings)?;
        app.screen = Screen::Practice(Session::new(prompt, settings.duration));
        app.fixture = Some((source.to_owned(), settings.mode));
        app.pending_code = false;
        app.exercise = if settings.mode == Mode::Words {
            Exercise::words()
        } else {
            Exercise::fixture()
        };
        Ok(app)
    }

    fn prepare(&mut self) -> Result<Session, PromptError> {
        self.session_id = self.selector.session_id();
        self.history.save = if self.history.enabled {
            SaveStatus::Pending
        } else {
            SaveStatus::Disabled
        };
        #[cfg(feature = "test-support")]
        if let Some((source, mode)) = &self.fixture
            && *mode == self.settings.mode
        {
            self.pending_code = false;
            self.exercise = if *mode == Mode::Words {
                Exercise::words()
            } else {
                Exercise::fixture()
            };
            return Ok(Session::new(
                Prompt::new(source, *mode)?,
                self.settings.duration,
            ));
        }
        let prepared = match self.settings.mode {
            Mode::Words => Prepared {
                meta: Exercise::words(),
                source: self.selector.words(),
            },
            Mode::Code => self.selector.code(),
        };
        self.exercise = prepared.meta;
        self.pending_code = self.settings.mode == Mode::Code;
        Ok(Session::new(
            crate::engine::prompt::Prompt::new(&prepared.source, self.settings.mode)?,
            self.settings.duration,
        ))
    }

    pub fn take_completion(&mut self) -> Option<Completion> {
        self.completion.take()
    }
    pub fn accept_reserved(&mut self, prepared: Prepared) -> Result<(), PromptError> {
        if self.pending_code
            && matches!(&self.screen,Screen::Practice(s) if s.phase()==Phase::Ready)
        {
            self.screen = Screen::Practice(Session::new(
                crate::engine::prompt::Prompt::new(&prepared.source, Mode::Code)?,
                self.settings.duration,
            ));
            self.exercise = prepared.meta;
        }
        self.pending_code = false;
        Ok(())
    }

    pub fn is_small(&self) -> bool {
        self.width < MIN_WIDTH || self.height < MIN_HEIGHT
    }
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }
    pub fn is_running(&self) -> bool {
        matches!(&self.screen, Screen::Practice(s) if matches!(s.phase(), Phase::Running { .. }))
    }
    pub fn remaining(&self, now: Duration) -> Option<Duration> {
        match &self.screen {
            Screen::Practice(s) if self.is_running() => Some(s.remaining(now)),
            _ => None,
        }
    }

    pub fn tick(&mut self, now: Duration) -> bool {
        if let Screen::Practice(session) = &mut self.screen {
            session.advance_time(now);
            if let Some(report) = session.report() {
                self.completion = Some(Completion {
                    id: self.session_id.clone(),
                    report: report.clone(),
                    exercise: self.exercise.clone(),
                });
                self.history.save = if self.history.enabled {
                    SaveStatus::Pending
                } else {
                    SaveStatus::Disabled
                };
                self.screen = Screen::Results(report);
                self.abandon = false;
                self.notice = None;
                return true;
            }
        }
        false
    }

    pub fn dispatch(&mut self, action: Action, now: Duration) -> Result<(), PromptError> {
        let expired = self.tick(now);
        match action {
            Action::Quit => {
                self.quit = true;
                return Ok(());
            }
            Action::Resize(w, h) => {
                self.set_size(w, h);
                return Ok(());
            }
            Action::PasteRejected => {
                self.notice = Some("Paste ignored. Type the exercise yourself.");
                return Ok(());
            }
            Action::SuspendRejected => {
                self.notice = Some("Suspend is unavailable. Ctrl+C exits safely.");
                return Ok(());
            }
            _ => {}
        }
        // An action intended for the active test cannot become a results-screen
        // command just because its processing time coincides with expiration.
        if expired || self.is_small() {
            return Ok(());
        }
        if matches!(self.screen, Screen::History { .. }) {
            self.history_action(action);
            return Ok(());
        }
        if action == Action::History
            && !self.is_running()
            && matches!(self.screen, Screen::Practice(_) | Screen::Results(_))
        {
            if !self.history.enabled {
                self.notice = Some("History disabled for this run.");
                return Ok(());
            }
            let previous = std::mem::replace(&mut self.screen, Screen::Transition);
            self.screen = Screen::History {
                previous: Box::new(previous),
            };
            self.history.filter.mode = Some(self.settings.mode);
            self.history.filter.seconds = Some(self.settings.duration.seconds());
            self.history.filter.exercise = None;
            self.history.filter.exercise_version = None;
            self.history.filter.reason = None;
            self.history.filter.language = None;
            self.history.scope = 0;
            self.history.tab = HistoryTab::Recent;
            self.history.reset_page();
            return Ok(());
        }
        if self.abandon {
            match action {
                Action::Confirm | Action::Text('y' | 'Y') => {
                    self.screen = Screen::Practice(self.prepare()?);
                    self.abandon = false;
                }
                Action::Cancel | Action::Text('n' | 'N') => self.abandon = false,
                _ => {}
            }
            return Ok(());
        }
        self.notice = None;
        match &mut self.screen {
            Screen::Practice(session) => {
                let ready = session.phase() == Phase::Ready;
                match action {
                    Action::Cancel if ready => self.open_settings()?,
                    Action::Cancel => self.abandon = true,
                    Action::ToggleMode if ready => {
                        self.settings.mode = self.settings.mode.other();
                        self.screen = Screen::Practice(self.prepare()?);
                    }
                    Action::CycleDuration if ready => {
                        self.settings.duration = self.settings.duration.cycle(true);
                        self.screen = Screen::Practice(self.prepare()?);
                    }
                    Action::Text(c) => {
                        if session.apply(EngineInput::Character(c), now)
                            == InputOutcome::Unsupported
                        {
                            self.notice = Some("Exercises use ASCII characters in this version.");
                        }
                        if session.settings().mode == Mode::Words
                            && session
                                .prompt()
                                .expected()
                                .len()
                                .saturating_sub(session.cursor())
                                < 32
                        {
                            session.append_words(&self.selector.words())?;
                        }
                    }
                    Action::Backspace => {
                        session.apply(EngineInput::Backspace, now);
                    }
                    _ => {}
                }
            }
            Screen::Settings { draft, field, .. } => match action {
                Action::NextField | Action::PreviousField => *field = 1 - *field,
                Action::Left | Action::Right => {
                    if *field == 0 {
                        draft.mode = draft.mode.other();
                    } else {
                        draft.duration = draft.duration.cycle(action == Action::Right);
                    }
                }
                Action::ToggleMode => draft.mode = draft.mode.other(),
                Action::CycleDuration => draft.duration = draft.duration.cycle(true),
                Action::Confirm => {
                    self.settings = *draft;
                    self.screen = Screen::Practice(self.prepare()?);
                }
                Action::Cancel => {
                    // Move the original prepared session back without rebuilding or starting it.
                    let replacement = Screen::Transition;
                    if let Screen::Settings { original, .. } =
                        std::mem::replace(&mut self.screen, replacement)
                    {
                        self.screen = Screen::Practice(*original);
                    }
                }
                _ => {}
            },
            Screen::Results(_) => match action {
                Action::Confirm => {
                    self.screen = Screen::Practice(self.prepare()?);
                }
                Action::Cancel => self.open_settings()?,
                Action::Text('r' | 'R') => self.history.retry = true,
                _ => {}
            },
            Screen::History { .. } | Screen::Transition => {}
        }
        self.tick(now);
        Ok(())
    }

    fn open_settings(&mut self) -> Result<(), PromptError> {
        let replacement = Screen::Transition;
        let old = std::mem::replace(&mut self.screen, replacement);
        let original = match old {
            Screen::Practice(session) => session,
            _ => self.prepare()?,
        };
        self.screen = Screen::Settings {
            draft: self.settings,
            field: 0,
            original: Box::new(original),
        };
        Ok(())
    }

    fn history_action(&mut self, action: Action) {
        if self.history.confirm_clear {
            match action {
                Action::Text('y' | 'Y') => {
                    self.history.clear_requested = true;
                    self.history.confirm_clear = false;
                }
                Action::Confirm | Action::Cancel | Action::Text('n' | 'N') => {
                    self.history.confirm_clear = false
                }
                _ => {}
            }
            return;
        }
        match action {
            Action::Cancel => {
                let old = std::mem::replace(&mut self.screen, Screen::Transition);
                if let Screen::History { previous } = old {
                    self.screen = *previous;
                }
            }
            Action::History => {
                self.history.tab = if self.history.tab == HistoryTab::Recent {
                    HistoryTab::Progress
                } else {
                    HistoryTab::Recent
                }
            }
            Action::ToggleMode => {
                self.history.filter.mode =
                    Some(self.history.filter.mode.unwrap_or(Mode::Words).other());
                self.history.filter.exercise = None;
                self.history.filter.exercise_version = None;
                self.history.filter.reason = None;
                self.history.filter.language = None;
                self.history.scope = 0;
                self.history.reset_page();
            }
            Action::CycleDuration => {
                let limit = crate::engine::TestDuration::from_seconds(
                    self.history.filter.seconds.unwrap_or(60),
                )
                .unwrap_or_default();
                self.history.filter.seconds = Some(limit.cycle(true).seconds());
                self.history.reset_page();
            }
            Action::Period => {
                self.history.filter.days = match self.history.filter.days {
                    None => Some(7),
                    Some(7) => Some(30),
                    _ => None,
                };
                self.history.reset_page();
            }
            Action::Scope if self.history.filter.mode == Some(Mode::Code) => {
                self.history.scope = (self.history.scope + 1) % 5;
                if self.history.scope == 1 && self.exercise.pool != "code" {
                    self.history.scope = 2;
                }
                self.history.filter.exercise = None;
                self.history.filter.exercise_version = None;
                self.history.filter.language = None;
                self.history.filter.reason = None;
                match self.history.scope {
                    1 => {
                        self.history.filter.exercise = Some(self.exercise.id.clone());
                        self.history.filter.exercise_version = Some(self.exercise.version);
                        self.history.filter.reason = Some(match &self.screen {
                            Screen::History { previous } => match previous.as_ref() {
                                Screen::Results(r) => r.reason,
                                _ => crate::engine::FinishReason::SnippetExhausted,
                            },
                            _ => crate::engine::FinishReason::SnippetExhausted,
                        });
                    }
                    2 => self.history.filter.language = Some("Rust".into()),
                    3 => self.history.filter.language = Some("Python".into()),
                    4 => self.history.filter.language = Some("JavaScript".into()),
                    _ => {}
                }
                self.history.reset_page();
            }
            Action::Graph => self.history.graph = (self.history.graph + 1) % 3,
            Action::ClearHistory => self.history.confirm_clear = true,
            Action::NextField => {
                self.history.scroll =
                    (self.history.scroll + 1).min(self.history.page.rows.len().saturating_sub(1))
            }
            Action::PreviousField => self.history.scroll = self.history.scroll.saturating_sub(1),
            Action::Right => {
                if let Some(next) = self.history.page.next.clone() {
                    self.history.cursors.push(Some(next));
                    self.history.scroll = 0;
                    self.history.reload = true;
                }
            }
            Action::Left if self.history.cursors.len() > 1 => {
                self.history.cursors.pop();
                self.history.scroll = 0;
                self.history.reload = true;
            }
            _ => {}
        }
    }
}
