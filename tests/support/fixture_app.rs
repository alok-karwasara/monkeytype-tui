use monkeytype_tui::{
    app::App,
    engine::{Mode, Settings, TestDuration},
    runtime::{FailurePoint, run_fixture, run_fixture_with_history},
};
use std::process::ExitCode;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let fixture = args.next().ok_or("fixture name required")?;
    let duration = args
        .next()
        .and_then(|s| s.parse().ok())
        .and_then(TestDuration::from_seconds)
        .ok_or("duration must be 30, 60, or 120")?;
    let (mode, source) = match fixture.as_str() {
        "short" => (Mode::Code, "abc"),
        "lines" => (Mode::Code, "a;\n    b c;"),
        "space" => (Mode::Code, "a b;"),
        "long" => (Mode::Code, "abcd"),
        "words" => (Mode::Words, "red blue"),
        _ => return Err("unknown fixture".into()),
    };
    let variant = args.next();
    if variant.as_deref() == Some("history") {
        let path = args.next().ok_or("history path required")?;
        return run_fixture_with_history(
            App::with_fixture(Settings { mode, duration }, source)?,
            None,
            path.into(),
        );
    }
    let failure = match variant.as_deref() {
        None => None,
        Some("init") => Some(FailurePoint::AfterInitialization),
        Some("partial-init") => Some(FailurePoint::PartialInitialization),
        Some("read") => Some(FailurePoint::Read),
        Some("draw") => Some(FailurePoint::Draw),
        Some("panic") => Some(FailurePoint::Panic),
        _ => return Err("unknown failure variant".into()),
    };
    run_fixture(
        App::with_fixture(Settings { mode, duration }, source)?,
        failure,
    )
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fixture-app: {error}");
            ExitCode::FAILURE
        }
    }
}
