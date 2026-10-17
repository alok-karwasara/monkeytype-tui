#[cfg(feature = "test-support")]
use crate::engine::prompt::Prompt;
use crate::{
    content,
    engine::{
        EngineInput, InputOutcome, Mode, Phase, Report, Session, Settings, prompt::PromptError,
    },
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
    sequence: usize,
    word_chunk: usize,
    #[cfg(feature = "test-support")]
    fixture: Option<(String, Mode)>,
}

impl App {
    pub fn new(settings: Settings) -> Result<Self, PromptError> {
        let prompt = content::prepare(settings, 0)?;
        Ok(Self {
            screen: Screen::Practice(Session::new(prompt, settings.duration)),
            settings,
            abandon: false,
            notice: None,
            quit: false,
            width: 80,
            height: 24,
            sequence: 0,
            word_chunk: 0,
            #[cfg(feature = "test-support")]
            fixture: None,
        })
    }

    #[cfg(feature = "test-support")]
    pub fn with_fixture(settings: Settings, source: &str) -> Result<Self, PromptError> {
        let prompt = Prompt::new(source, settings.mode)?;
        let mut app = Self::new(settings)?;
        app.screen = Screen::Practice(Session::new(prompt, settings.duration));
        app.fixture = Some((source.to_owned(), settings.mode));
        Ok(app)
    }

    fn prepare(&mut self) -> Result<Session, PromptError> {
        self.word_chunk = 0;
        #[cfg(feature = "test-support")]
        if let Some((source, mode)) = &self.fixture
            && *mode == self.settings.mode
        {
            return Ok(Session::new(
                Prompt::new(source, *mode)?,
                self.settings.duration,
            ));
        }
        Ok(Session::new(
            content::prepare(self.settings, self.sequence)?,
            self.settings.duration,
        ))
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
        if self.abandon {
            match action {
                Action::Confirm | Action::Text('y' | 'Y') => {
                    self.sequence = self.sequence.wrapping_add(1);
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
                            self.word_chunk += 1;
                            session.append_words(&content::word_chunk(
                                self.sequence,
                                self.word_chunk,
                            ))?;
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
                    let replacement = Screen::Practice(self.prepare()?);
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
                    self.sequence = self.sequence.wrapping_add(1);
                    self.screen = Screen::Practice(self.prepare()?);
                }
                Action::Cancel => self.open_settings()?,
                _ => {}
            },
        }
        self.tick(now);
        Ok(())
    }

    fn open_settings(&mut self) -> Result<(), PromptError> {
        let replacement = Screen::Practice(self.prepare()?);
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
}
