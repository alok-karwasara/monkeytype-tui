use crossterm::{
    cursor::Show,
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
    style::{Attribute, ResetColor, SetAttribute},
    terminal::LeaveAlternateScreen,
};
use std::io::{self, IsTerminal};

pub struct TerminalSession {
    pub terminal: Option<ratatui::DefaultTerminal>,
    active: bool,
}

impl TerminalSession {
    fn cleanup_scope() -> io::Result<Self> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "an interactive terminal is required (run without piping input/output)",
            ));
        }
        // Construct the cleanup scope before any fallible terminal initialization.
        Ok(Self {
            terminal: None,
            active: true,
        })
    }

    pub fn enter() -> io::Result<Self> {
        let mut session = Self::cleanup_scope()?;
        session.terminal = Some(ratatui::try_init()?);
        execute!(io::stdout(), EnableBracketedPaste)?;
        Ok(session)
    }

    #[cfg(feature = "test-support")]
    pub fn exercise_partial_initialization_failure() -> io::Result<()> {
        let _cleanup = Self::cleanup_scope()?;
        crossterm::terminal::enable_raw_mode()?;
        Err(io::Error::other(
            "injected failure after raw mode, before alternate screen",
        ))
    }

    pub fn close(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let base = ratatui::try_restore();
        // Attempt all extra restoration even if raw-mode restoration failed.
        let extra = execute!(
            io::stdout(),
            DisableBracketedPaste,
            Show,
            SetAttribute(Attribute::Reset),
            ResetColor,
            LeaveAlternateScreen
        );
        base.and(extra)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
