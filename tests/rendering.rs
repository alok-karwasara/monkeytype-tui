use monkeytype_tui::{
    app::{App, Screen},
    engine::{Mode, Session, Settings, TestDuration, prompt::Prompt},
    input::Action,
    ui,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier},
};
use std::{error::Error, time::Duration};

type TestResult = Result<(), Box<dyn Error>>;
fn app() -> Result<App, Box<dyn Error>> {
    let mut app = App::new(Settings {
        mode: Mode::Code,
        duration: TestDuration::Seconds30,
    })?;
    app.screen = Screen::Practice(Session::new(
        Prompt::new("a;\n    b c;", Mode::Code)?,
        TestDuration::Seconds30,
    ));
    Ok(app)
}
fn render(app: &App, width: u16, height: u16) -> Result<TestBackend, Box<dyn Error>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| ui::draw(frame, app, Duration::from_secs(1)))?;
    Ok(terminal.backend().clone())
}
fn contents(backend: &TestBackend) -> String {
    backend
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn ui01_ui02_instrument_uses_terminal_defaults() -> TestResult {
    let b = render(&app()?, 100, 30)?;
    let text = contents(&b);
    for label in [
        "monkeytype-tui",
        "code / 30s",
        "TIME",
        "WPM",
        "ACCURACY",
        "ERRORS",
        "READY",
        "F2 mode",
    ] {
        assert!(text.contains(label), "missing {label}");
    }
    assert!(b.buffer().content.iter().all(|c| c.bg == Color::Reset));
    Ok(())
}

#[test]
fn ui03_error_is_underlined_not_only_colored() -> TestResult {
    let mut a = app()?;
    a.dispatch(Action::Text('x'), Duration::ZERO)?;
    let b = render(&a, 100, 30)?;
    assert!(
        b.buffer()
            .content
            .iter()
            .any(|c| c.symbol() == "x" && c.modifier.contains(Modifier::UNDERLINED))
    );
    Ok(())
}

#[test]
fn ui04_caret_skips_indentation_and_keeps_internal_space() -> TestResult {
    let mut a = app()?;
    for c in "a;".chars() {
        a.dispatch(Action::Text(c), Duration::ZERO)?;
    }
    let b = render(&a, 100, 30)?;
    assert!(
        b.buffer()
            .content
            .iter()
            .any(|c| c.symbol() == "b" && c.modifier.contains(Modifier::REVERSED))
    );
    a.dispatch(Action::Text('b'), Duration::ZERO)?;
    let b = render(&a, 100, 30)?;
    assert!(
        b.buffer()
            .content
            .iter()
            .any(|c| c.symbol() == " " && c.modifier.contains(Modifier::REVERSED))
    );
    Ok(())
}

#[test]
fn ui05_ui06_normal_narrow_tiny_and_empty_frames_are_safe() -> TestResult {
    let a = app()?;
    for (width, height) in [(100, 30), (60, 18), (40, 12), (1, 1), (0, 0)] {
        render(&a, width, height)?;
    }
    assert!(contents(&render(&a, 30, 10)?).contains("Resize"));
    assert!(contents(&render(&a, 40, 12)?).contains("^C quit"));
    Ok(())
}

#[test]
fn ui08_long_formatted_and_wrapped_prompts_keep_caret_visible() -> TestResult {
    for (mode, source) in [
        (
            Mode::Code,
            (0..80)
                .map(|i| format!("    line_{i}();\n"))
                .collect::<String>(),
        ),
        (Mode::Words, "red blue green gold ".repeat(80)),
    ] {
        let prompt = Prompt::new(&source, mode)?;
        let input: Vec<char> = prompt
            .expected()
            .iter()
            .take(400)
            .map(|u| u.character)
            .collect();
        let mut session = Session::new(prompt, TestDuration::Seconds60);
        for c in input {
            session.apply(
                monkeytype_tui::engine::EngineInput::Character(c),
                Duration::ZERO,
            );
        }
        let next = session.prompt().expected()[session.cursor()]
            .character
            .to_string();
        let mut a = App::new(session.settings())?;
        a.screen = Screen::Practice(session);
        let b = render(&a, 40, 12)?;
        assert!(
            b.buffer()
                .content
                .iter()
                .any(|c| c.symbol() == next && c.modifier.contains(Modifier::REVERSED))
        );
    }
    Ok(())
}

#[test]
fn ui09_ui10_results_settings_and_abandon_are_readable() -> TestResult {
    let mut a = app()?;
    a.dispatch(Action::Cancel, Duration::ZERO)?;
    assert!(contents(&render(&a, 100, 30)?).contains("SETTINGS"));
    a.dispatch(Action::Cancel, Duration::ZERO)?;
    a.dispatch(Action::Text('a'), Duration::ZERO)?;
    a.dispatch(Action::Cancel, Duration::ZERO)?;
    assert!(contents(&render(&a, 100, 30)?).contains("CONFIRM ABANDON"));
    a.tick(Duration::from_secs(30));
    assert!(contents(&render(&a, 100, 30)?).contains("TEST COMPLETE"));
    Ok(())
}
