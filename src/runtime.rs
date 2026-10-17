use crate::{
    app::App,
    engine::{Mode, Settings, TestDuration},
    input,
    terminal::TerminalSession,
    ui,
};
use crossterm::event;
use std::{
    error::Error,
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

type AppResult<T> = Result<T, Box<dyn Error>>;

const HELP: &str = "monkeytype-tui — offline words and code typing\n\nUsage: monkeytype-tui [--mode words|code] [--duration 30|60|120]\n\nReady: F2 changes mode; F3 changes duration; Esc opens settings.\nSettings: arrows/Tab select; Enter applies; Esc cancels.\nPractice: type normally; Backspace corrects; Esc offers abandon.\nCtrl+C quits safely from every screen.\n\nCode indentation, trailing spaces, and newlines are automatic.\nSpaces inside each line must be typed. Your terminal controls colors/font.\n";

pub fn parse_settings(args: impl Iterator<Item = String>) -> AppResult<Option<Settings>> {
    let mut settings = Settings::default();
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                return Ok(None);
            }
            "--version" | "-V" => {
                println!("monkeytype-tui {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--mode" => {
                settings.mode = match args.next().as_deref() {
                    Some("words") => Mode::Words,
                    Some("code") => Mode::Code,
                    _ => return Err("--mode requires words or code".into()),
                };
            }
            "--duration" => {
                let value = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .and_then(TestDuration::from_seconds);
                settings.duration = value.ok_or("--duration requires 30, 60, or 120 seconds")?;
            }
            _ => return Err(format!("unknown argument {arg:?}; use --help").into()),
        }
    }
    Ok(Some(settings))
}

pub fn main_entry(args: impl Iterator<Item = String>) -> AppResult<()> {
    let Some(settings) = parse_settings(args)? else {
        return Ok(());
    };
    run(App::new(settings)?)
}

struct Shutdown {
    requested: Arc<AtomicBool>,
    registrations: Vec<signal_hook::SigId>,
}

impl Shutdown {
    fn new() -> io::Result<Self> {
        let mut signals = Self {
            requested: Arc::new(AtomicBool::new(false)),
            registrations: Vec::new(),
        };
        for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
            signals.registrations.push(signal_hook::flag::register(
                signal,
                Arc::clone(&signals.requested),
            )?);
        }
        Ok(signals)
    }
}

impl Drop for Shutdown {
    fn drop(&mut self) {
        for id in self.registrations.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}

pub fn run(app: App) -> AppResult<()> {
    run_inner(app, None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailurePoint {
    PartialInitialization,
    AfterInitialization,
    Read,
    Draw,
    Panic,
}

#[cfg(feature = "test-support")]
pub fn run_fixture(app: App, failure: Option<FailurePoint>) -> AppResult<()> {
    run_inner(app, failure)
}

fn run_inner(mut app: App, failure: Option<FailurePoint>) -> AppResult<()> {
    let shutdown = Shutdown::new()?;
    #[cfg(feature = "test-support")]
    if failure == Some(FailurePoint::PartialInitialization) {
        return TerminalSession::exercise_partial_initialization_failure().map_err(Into::into);
    }
    let mut scope = TerminalSession::enter()?;
    let result = event_loop(&mut scope, &mut app, &shutdown, failure);
    let cleanup = scope.close();
    result.and_then(|()| cleanup.map_err(Into::into))
}

fn event_loop(
    scope: &mut TerminalSession,
    app: &mut App,
    shutdown: &Shutdown,
    failure: Option<FailurePoint>,
) -> AppResult<()> {
    let terminal = scope
        .terminal
        .as_mut()
        .ok_or("terminal initialization did not finish")?;
    let size = terminal.size()?;
    app.set_size(size.width, size.height);
    if failure == Some(FailurePoint::AfterInitialization) {
        return Err(io::Error::other("injected initialization failure").into());
    }
    let clock = Instant::now();
    let mut next_paint = Duration::ZERO;
    let mut dirty = true;
    loop {
        if app.quit || shutdown.requested.load(Ordering::Relaxed) {
            return Ok(());
        }
        let now = clock.elapsed();
        dirty |= app.tick(now);
        if now >= next_paint && (dirty || app.is_running()) {
            if failure == Some(FailurePoint::Draw) {
                return Err(io::Error::other("injected draw failure").into());
            }
            terminal.draw(|frame| ui::draw(frame, app, now))?;
            next_paint = now + Duration::from_millis(33);
            dirty = false;
            if failure == Some(FailurePoint::Panic) {
                panic!("injected unwinding panic");
            }
        }
        let now = clock.elapsed();
        let mut wait = if dirty || app.is_running() {
            next_paint.saturating_sub(now)
        } else {
            Duration::from_millis(250)
        };
        if let Some(remaining) = app.remaining(now) {
            wait = wait.min(remaining);
        }
        if failure == Some(FailurePoint::Read) {
            return Err(io::Error::other("injected read failure").into());
        }
        if event::poll(wait)? {
            let event = event::read()?;
            if let Some(action) = input::normalize(event) {
                // A draw can observe the new size before Crossterm delivers Resize.
                // Never score input against stale dimensions while showing fallback.
                let size = terminal.size()?;
                app.set_size(size.width, size.height);
                app.dispatch(action, clock.elapsed())?;
                dirty = true;
            } else {
                dirty |= app.tick(clock.elapsed());
            }
        }
    }
}
