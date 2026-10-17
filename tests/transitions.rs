use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use monkeytype_tui::{
    app::{App, Screen},
    engine::{Mode, Phase, Session, Settings, TestDuration, prompt::Prompt},
    input::{Action, normalize},
    runtime::parse_settings,
};
use std::{error::Error, time::Duration};

type TestResult = Result<(), Box<dyn Error>>;
fn fixture() -> Result<App, Box<dyn Error>> {
    let settings = Settings {
        mode: Mode::Code,
        duration: TestDuration::Seconds30,
    };
    let mut app = App::new(settings)?;
    app.screen = Screen::Practice(Session::new(
        Prompt::new("abcdef", Mode::Code)?,
        settings.duration,
    ));
    Ok(app)
}

#[test]
fn in01_in02_in03_press_release_repeat() {
    for (kind, expected) in [
        (KeyEventKind::Press, Some(Action::Text('a'))),
        (KeyEventKind::Release, None),
        (KeyEventKind::Repeat, Some(Action::Text('a'))),
    ] {
        assert_eq!(
            normalize(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char('a'),
                KeyModifiers::NONE,
                kind
            ))),
            expected
        );
    }
    assert_eq!(
        normalize(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Repeat
        ))),
        None
    );
}

#[test]
fn in05_printable_code_symbols_are_never_shortcuts() {
    for c in "q:[]{}()".chars() {
        assert_eq!(
            normalize(Event::Key(KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::NONE
            ))),
            Some(Action::Text(c))
        );
    }
    assert_eq!(
        normalize(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        ))),
        Some(Action::Quit)
    );
    assert_eq!(
        normalize(Event::Key(KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::CONTROL
        ))),
        Some(Action::SuspendRejected)
    );
}

#[test]
fn in06_in07_ignored_formatting_paste_and_unicode_do_not_start() -> TestResult {
    let mut app = fixture()?;
    for action in [
        Action::Confirm,
        Action::NextField,
        Action::PasteRejected,
        Action::Text('é'),
    ] {
        app.dispatch(action, Duration::from_secs(20))?;
    }
    match &app.screen {
        Screen::Practice(s) => {
            assert_eq!(s.phase(), Phase::Ready);
            assert_eq!(s.counters().attempts, 0);
        }
        _ => panic!("unexpected screen"),
    }
    assert!(app.notice.is_some());
    Ok(())
}

#[test]
fn app01_app02_all_mode_duration_combinations() -> TestResult {
    for mode in [Mode::Words, Mode::Code] {
        for duration in [
            TestDuration::Seconds30,
            TestDuration::Seconds60,
            TestDuration::Seconds120,
        ] {
            let mut app = App::new(Settings::default())?;
            app.dispatch(Action::Cancel, Duration::ZERO)?;
            if let Screen::Settings { draft, .. } = &mut app.screen {
                *draft = Settings { mode, duration };
            }
            app.dispatch(Action::Confirm, Duration::ZERO)?;
            assert_eq!(app.settings, Settings { mode, duration });
            assert!(
                matches!(&app.screen, Screen::Practice(s) if s.phase() == Phase::Ready && s.settings() == app.settings)
            );
        }
    }
    Ok(())
}

#[test]
fn app03_settings_cancel_preserves_original_prepared_prompt() -> TestResult {
    let mut app = fixture()?;
    app.dispatch(Action::Cancel, Duration::ZERO)?;
    app.dispatch(Action::Right, Duration::ZERO)?;
    app.dispatch(Action::CycleDuration, Duration::ZERO)?;
    app.dispatch(Action::Cancel, Duration::ZERO)?;
    assert_eq!(app.settings.mode, Mode::Code);
    assert_eq!(app.settings.duration, TestDuration::Seconds30);
    assert!(
        matches!(&app.screen, Screen::Practice(s) if s.prompt().display() == "abcdef" && s.phase() == Phase::Ready)
    );
    Ok(())
}

#[test]
fn app04_app05_abandon_cancel_and_confirm() -> TestResult {
    let mut app = fixture()?;
    app.dispatch(Action::Text('a'), Duration::from_secs(10))?;
    app.dispatch(Action::Cancel, Duration::from_secs(12))?;
    assert!(app.abandon);
    app.dispatch(Action::Cancel, Duration::from_secs(14))?;
    assert!(!app.abandon);
    assert!(
        matches!(&app.screen, Screen::Practice(s) if s.cursor() == 1 && s.elapsed(Duration::from_secs(14)) == Some(Duration::from_secs(4)))
    );
    app.dispatch(Action::Cancel, Duration::from_secs(15))?;
    app.dispatch(Action::Confirm, Duration::from_secs(16))?;
    assert!(
        matches!(&app.screen, Screen::Practice(s) if s.cursor() == 0 && s.phase() == Phase::Ready)
    );
    Ok(())
}

#[test]
fn time12_app08_timeout_in_overlay_cannot_resume_or_accept_input() -> TestResult {
    let mut app = fixture()?;
    app.dispatch(Action::Text('a'), Duration::ZERO)?;
    app.dispatch(Action::Cancel, Duration::from_secs(29))?;
    app.dispatch(Action::Text('b'), Duration::from_secs(30))?;
    assert!(!app.abandon);
    assert!(
        matches!(&app.screen, Screen::Results(r) if r.counters.attempts == 1 && r.elapsed == Duration::from_secs(30))
    );
    let before = match &app.screen {
        Screen::Results(r) => r.clone(),
        _ => return Err("expected results".into()),
    };
    app.dispatch(Action::Backspace, Duration::from_secs(31))?;
    assert!(matches!(&app.screen, Screen::Results(r) if r == &before));
    app.dispatch(Action::Confirm, Duration::from_secs(32))?;
    assert!(
        matches!(&app.screen, Screen::Practice(s) if s.phase() == Phase::Ready && s.counters().attempts == 0)
    );
    Ok(())
}

#[test]
fn ui07_tiny_window_ignores_text_but_not_time_or_quit() -> TestResult {
    let mut app = fixture()?;
    app.dispatch(Action::Text('a'), Duration::ZERO)?;
    app.dispatch(Action::Resize(1, 1), Duration::from_secs(1))?;
    app.dispatch(Action::Text('x'), Duration::from_secs(2))?;
    app.dispatch(Action::Resize(80, 24), Duration::from_secs(3))?;
    assert!(
        matches!(&app.screen, Screen::Practice(s) if s.cursor() == 1 && s.elapsed(Duration::from_secs(3)) == Some(Duration::from_secs(3)))
    );
    app.dispatch(Action::Quit, Duration::from_secs(4))?;
    assert!(app.quit);
    Ok(())
}

#[test]
fn app08_deadline_action_cannot_be_reinterpreted_as_result_navigation() -> TestResult {
    for action in [
        Action::Confirm,
        Action::Cancel,
        Action::Backspace,
        Action::Text('b'),
    ] {
        let mut app = fixture()?;
        app.dispatch(Action::Text('a'), Duration::ZERO)?;
        app.dispatch(action, Duration::from_secs(30))?;
        assert!(matches!(&app.screen, Screen::Results(r) if r.counters.attempts == 1));
    }
    Ok(())
}

#[test]
fn cli_rejects_unsupported_options_without_starting_terminal() -> TestResult {
    for args in [
        vec!["--duration", "2"],
        vec!["--mode", "editor"],
        vec!["--unknown"],
        vec!["--duration"],
    ] {
        assert!(parse_settings(args.into_iter().map(str::to_owned)).is_err());
    }
    let settings = parse_settings(
        ["--mode", "code", "--duration", "120"]
            .into_iter()
            .map(str::to_owned),
    )?
    .ok_or("missing settings")?;
    assert_eq!(
        settings,
        Settings {
            mode: Mode::Code,
            duration: TestDuration::Seconds120
        }
    );
    Ok(())
}
