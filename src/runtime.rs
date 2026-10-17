use crate::{
    app::App,
    engine::{Mode, Settings, TestDuration},
    history::{self, service::Service},
    input,
    terminal::TerminalSession,
    ui,
};
use crossterm::event;
use std::{
    error::Error,
    io,
    io::{BufRead, Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

type AppResult<T> = Result<T, Box<dyn Error>>;

const HELP: &str = "monkeytype-tui — offline words and code typing\n\nUsage: monkeytype-tui [--mode words|code] [--duration 30|60|120]\n                     [--no-history] [--history-dir /absolute/path]\n       monkeytype-tui history purge [--history-dir /absolute/path]\n\nReady: F2 mode; F3 duration; F4 history; Esc settings.\nHistory: F4 recent/progress; F2/F3 filters; F5 period; F6 code scope; F7 graph; F8 clear.\nResults: Enter restart; r retry unsaved results; F4 history.\nSettings: arrows/Tab select; Enter applies; Esc cancels.\nPractice: type normally; Backspace corrects; Esc offers abandon.\nCtrl+C quits safely from every screen.\n\nOnly completed aggregate results are saved locally; typed text is never stored.\n--no-history performs no history filesystem access. Uninstall preserves data;\npurge requires explicit confirmation and refuses a running history instance.\nCode indentation, trailing spaces, and newlines are automatic.\nSpaces inside each line must be typed. Your terminal controls colors/font.\n";

#[derive(Debug)]
pub struct Options {
    pub settings: Settings,
    pub history_enabled: bool,
    pub history_dir: Option<PathBuf>,
    pub purge: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            history_enabled: true,
            history_dir: None,
            purge: false,
        }
    }
}
impl Options {
    pub fn directory(&self) -> AppResult<Option<PathBuf>> {
        if !self.history_enabled {
            return Ok(None);
        }
        if self.history_dir.is_some() {
            return history::resolve_directory(
                true,
                self.history_dir.clone(),
                None,
                None,
                cfg!(target_os = "macos"),
            );
        }
        // Read only these named nonsecret path variables, never enumerate or read files.
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let xdg = if cfg!(target_os = "macos") {
            None
        } else {
            std::env::var_os("XDG_STATE_HOME").map(PathBuf::from)
        };
        history::resolve_directory(true, None, home, xdg, cfg!(target_os = "macos"))
    }
}

pub fn parse_settings(args: impl Iterator<Item = String>) -> AppResult<Option<Settings>> {
    Ok(parse_options(args)?.map(|o| o.settings))
}
pub fn parse_options(args: impl Iterator<Item = String>) -> AppResult<Option<Options>> {
    let mut options = Options::default();
    let mut args = args.peekable();
    if args.peek().is_some_and(|a| a == "history") {
        args.next();
        if args.next().as_deref() != Some("purge") {
            return Err("usage: monkeytype-tui history purge [--history-dir PATH]".into());
        }
        options.purge = true;
    }
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
                if options.purge {
                    return Err("--mode is not a purge option".into());
                }
                options.settings.mode = match args.next().as_deref() {
                    Some("words") => Mode::Words,
                    Some("code") => Mode::Code,
                    _ => return Err("--mode requires words or code".into()),
                };
            }
            "--duration" => {
                if options.purge {
                    return Err("--duration is not a purge option".into());
                }
                let value = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .and_then(TestDuration::from_seconds);
                options.settings.duration =
                    value.ok_or("--duration requires 30, 60, or 120 seconds")?;
            }
            "--no-history" => options.history_enabled = false,
            "--history-dir" => {
                options.history_dir = Some(
                    args.next()
                        .filter(|s| !s.is_empty())
                        .map(PathBuf::from)
                        .ok_or("--history-dir requires a directory")?,
                )
            }
            _ => return Err(format!("unknown argument {arg:?}; use --help").into()),
        }
    }
    if options.purge && !options.history_enabled {
        return Err("purge cannot be combined with --no-history".into());
    }
    if options.history_enabled
        && options
            .history_dir
            .as_ref()
            .is_some_and(|p| !p.is_absolute())
    {
        return Err("--history-dir requires an absolute path".into());
    }
    Ok(Some(options))
}

pub fn main_entry(args: impl Iterator<Item = String>) -> AppResult<()> {
    let Some(options) = parse_options(args)? else {
        return Ok(());
    };
    if options.purge {
        let directory = options
            .directory()?
            .ok_or("history directory unavailable")?;
        print!("Purge all local history and selection state? Type PURGE to confirm: ");
        io::stdout().flush()?;
        let mut response = String::new();
        io::stdin().lock().take(64).read_line(&mut response)?;
        if response.trim() != "PURGE" {
            println!("Cancelled; history preserved.");
            return Ok(());
        }
        history::purge(&directory)?;
        println!("History purged. The empty coordination lock may remain.");
        return Ok(());
    }
    run_inner(App::new(options.settings)?, None, options)
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
    run_inner(app, None, Options::default())
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
    run_inner(
        app,
        failure,
        Options {
            history_enabled: false,
            ..Options::default()
        },
    )
}

#[cfg(feature = "test-support")]
pub fn run_fixture_with_history(
    app: App,
    failure: Option<FailurePoint>,
    directory: PathBuf,
) -> AppResult<()> {
    run_inner(
        app,
        failure,
        Options {
            history_dir: Some(directory),
            ..Options::default()
        },
    )
}

fn run_inner(mut app: App, failure: Option<FailurePoint>, options: Options) -> AppResult<()> {
    let shutdown = Shutdown::new()?;
    #[cfg(feature = "test-support")]
    if failure == Some(FailurePoint::PartialInitialization) {
        return TerminalSession::exercise_partial_initialization_failure().map_err(Into::into);
    }
    let mut scope = TerminalSession::enter()?;
    let mut history = Service::new(options.history_enabled, options.directory(), &mut app);
    history.settle(&mut app)?;
    let result = event_loop(&mut scope, &mut app, &shutdown, failure, &mut history);
    let cleanup = scope.close();
    result.and_then(|()| cleanup.map_err(Into::into))
}

fn event_loop(
    scope: &mut TerminalSession,
    app: &mut App,
    shutdown: &Shutdown,
    failure: Option<FailurePoint>,
    history: &mut Service,
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
        history.settle(app)?;
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
                history.settle(app)?;
                dirty = true;
            } else {
                dirty |= app.tick(clock.elapsed());
            }
        }
    }
}
