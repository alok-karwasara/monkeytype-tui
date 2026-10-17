//! Application/history contracts written before the feature implementation.
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use monkeytype_tui::{
    app::{App, Screen},
    engine::{Mode, Phase, Session, Settings, TestDuration, prompt::Prompt},
    history::{HistoryTab, SaveStatus},
    input::{self, Action},
    ui,
};
use ratatui::{Terminal, backend::TestBackend, style::Color};
use std::{error::Error, time::Duration};
type TestResult = Result<(), Box<dyn Error>>;
fn app() -> Result<App, Box<dyn Error>> {
    let mut a = App::new(Settings {
        mode: Mode::Code,
        duration: TestDuration::Seconds30,
    })?;
    a.screen = Screen::Practice(Session::new(
        Prompt::new("abc", Mode::Code)?,
        TestDuration::Seconds30,
    ));
    a.pending_code = false;
    a.exercise = monkeytype_tui::content::Exercise::fixture();
    Ok(a)
}
fn finish(a: &mut App) -> TestResult {
    for (i, c) in "abc".chars().enumerate() {
        a.dispatch(Action::Text(c), Duration::from_millis(i as u64 * 100))?;
    }
    Ok(())
}
fn text(a: &App, w: u16, h: u16) -> Result<String, Box<dyn Error>> {
    let mut t = Terminal::new(TestBackend::new(w, h))?;
    t.draw(|f| ui::draw(f, a, Duration::ZERO))?;
    assert!(
        t.backend()
            .buffer()
            .content
            .iter()
            .all(|c| c.bg == Color::Reset)
    );
    Ok(t.backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect())
}

#[test]
fn hap01_completion_effect_once_frozen_across_redraw_restart_and_quit() -> TestResult {
    let mut a = app()?;
    finish(&mut a)?;
    let effect = a.take_completion().ok_or("missing completion")?;
    assert_eq!(effect.report.counters.attempts, 3);
    assert!(a.take_completion().is_none());
    a.tick(Duration::from_secs(50));
    text(&a, 100, 30)?;
    a.dispatch(Action::Confirm, Duration::from_secs(51))?;
    a.dispatch(Action::Quit, Duration::from_secs(52))?;
    assert!(a.take_completion().is_none());
    assert_eq!(effect.report.counters.attempts, 3);
    Ok(())
}

#[test]
fn hap02_timeout_overlay_and_deadline_quit_emit_exactly_once() -> TestResult {
    for last in [
        Action::Confirm,
        Action::Cancel,
        Action::Quit,
        Action::History,
    ] {
        let mut a = app()?;
        a.dispatch(Action::Text('a'), Duration::ZERO)?;
        a.dispatch(Action::Cancel, Duration::from_secs(29))?;
        a.dispatch(last, Duration::from_secs(30))?;
        let e = a.take_completion().ok_or("missing timeout summary")?;
        assert_eq!(e.report.elapsed, Duration::from_secs(30));
        assert_eq!(e.report.counters.attempts, 1);
        assert!(a.take_completion().is_none());
        assert!(matches!(a.screen, Screen::Results(_)));
    }
    Ok(())
}

#[test]
fn hap03_abandon_unstarted_and_early_quit_never_emit_completed_record() -> TestResult {
    let mut a = app()?;
    a.dispatch(Action::Cancel, Duration::ZERO)?;
    a.dispatch(Action::Cancel, Duration::ZERO)?;
    assert!(a.take_completion().is_none());
    a.dispatch(Action::Text('a'), Duration::ZERO)?;
    a.dispatch(Action::Cancel, Duration::from_secs(1))?;
    a.dispatch(Action::Confirm, Duration::from_secs(2))?;
    assert!(a.take_completion().is_none());
    a.dispatch(Action::Quit, Duration::from_secs(3))?;
    assert!(a.take_completion().is_none());
    Ok(())
}

#[test]
fn hap04_history_ready_and_results_roundtrip_preserves_original() -> TestResult {
    let mut a = app()?;
    a.history.enabled = true;
    a.dispatch(Action::History, Duration::ZERO)?;
    assert!(matches!(a.screen, Screen::History { .. }));
    a.dispatch(Action::Cancel, Duration::ZERO)?;
    assert!(
        matches!(&a.screen,Screen::Practice(s) if s.phase()==Phase::Ready && s.prompt().display()=="abc")
    );
    finish(&mut a)?;
    let before = match &a.screen {
        Screen::Results(r) => r.clone(),
        _ => return Err("results expected".into()),
    };
    a.dispatch(Action::History, Duration::from_secs(1))?;
    a.dispatch(Action::Cancel, Duration::from_secs(2))?;
    assert!(matches!(&a.screen,Screen::Results(r) if *r==before));
    Ok(())
}

#[test]
fn hap05_history_disabled_and_active_test_do_not_open_or_change_timing() -> TestResult {
    let mut a = app()?;
    a.dispatch(Action::History, Duration::ZERO)?;
    assert!(matches!(a.screen, Screen::Practice(_)));
    assert_eq!(a.history.save, SaveStatus::Disabled);
    a.history.enabled = true;
    a.dispatch(Action::Text('a'), Duration::ZERO)?;
    a.dispatch(Action::History, Duration::from_secs(2))?;
    assert!(
        matches!(&a.screen,Screen::Practice(s) if s.cursor()==1 && s.elapsed(Duration::from_secs(2))==Some(Duration::from_secs(2)))
    );
    Ok(())
}

#[test]
fn hap06_progress_filters_and_clear_default_cancel() -> TestResult {
    let mut a = app()?;
    a.history.enabled = true;
    a.dispatch(Action::History, Duration::ZERO)?;
    a.dispatch(Action::History, Duration::ZERO)?;
    assert_eq!(a.history.tab, HistoryTab::Progress);
    a.dispatch(Action::ToggleMode, Duration::ZERO)?;
    assert_eq!(a.history.filter.mode, Some(Mode::Words));
    a.dispatch(Action::CycleDuration, Duration::ZERO)?;
    assert_eq!(a.history.filter.seconds, Some(60));
    a.dispatch(Action::ClearHistory, Duration::ZERO)?;
    assert!(a.history.confirm_clear);
    a.dispatch(Action::Confirm, Duration::ZERO)?;
    assert!(!a.history.confirm_clear);
    assert!(!a.history.clear_requested);
    a.dispatch(Action::ClearHistory, Duration::ZERO)?;
    a.dispatch(Action::Text('y'), Duration::ZERO)?;
    assert!(a.history.clear_requested);
    Ok(())
}

#[test]
fn hap07_history_progress_empty_flat_and_tiny_layouts_render() -> TestResult {
    let mut a = app()?;
    a.history.enabled = true;
    a.dispatch(Action::History, Duration::ZERO)?;
    assert!(text(&a, 100, 30)?.contains("HISTORY"));
    a.history.page.rows = vec![monkeytype_tui::history::Record::new(
        uuid::Uuid::new_v4().to_string(),
        0,
        Mode::Words,
        60,
        60_000_000_000,
        monkeytype_tui::history::Counts {
            attempts: 100,
            correct_attempts: 90,
            current_correct: 80,
            backspaces: 10,
            uncorrected: 10,
        },
    )];
    assert!(text(&a, 100, 30)?.contains("16.0/20.0"));
    a.dispatch(Action::History, Duration::ZERO)?;
    for points in [
        vec![],
        vec![(1.0, 16.0, 90.0)],
        vec![(1.0, 16.0, 90.0), (2.0, 16.0, 90.0)],
    ] {
        a.history.stats.trend = points;
        a.history.stats.daily = vec![0.0; 30];
        a.history.stats.daily[29] = 60.0;
        for graph in 0..3 {
            a.history.graph = graph;
            for (w, h) in [(100, 30), (60, 18), (40, 12), (1, 1), (0, 0)] {
                text(&a, w, h)?;
            }
        }
    }
    assert!(text(&a, 100, 30)?.contains("PROGRESS"));
    Ok(())
}

#[test]
fn hap08_menu_keys_ignore_repeats_and_releases() -> TestResult {
    use monkeytype_tui::runtime::parse_options;
    for args in [
        vec!["--history-dir"],
        vec!["--history-dir", "relative"],
        vec!["history", "other"],
        vec!["history", "purge", "--no-history"],
    ] {
        assert!(parse_options(args.into_iter().map(str::to_owned)).is_err());
    }
    let disabled = parse_options(
        ["--no-history", "--history-dir", "relative"]
            .into_iter()
            .map(str::to_owned),
    )?
    .ok_or("options")?;
    assert_eq!(disabled.directory()?, None);
    for (key, action) in [
        (4, Action::History),
        (5, Action::Period),
        (6, Action::Scope),
        (7, Action::Graph),
        (8, Action::ClearHistory),
    ] {
        assert_eq!(
            input::normalize(Event::Key(KeyEvent::new(
                KeyCode::F(key),
                KeyModifiers::NONE
            ))),
            Some(action)
        );
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            assert_eq!(
                input::normalize(Event::Key(KeyEvent::new_with_kind(
                    KeyCode::F(key),
                    KeyModifiers::NONE,
                    kind
                ))),
                None
            );
        }
    }
    Ok(())
}

#[test]
fn hap09_period_scope_metric_and_paging_boundaries() -> TestResult {
    let mut a = app()?;
    a.history.enabled = true;
    a.dispatch(Action::History, Duration::ZERO)?;
    a.history.page.next = Some((123, "cursor".into()));
    a.dispatch(Action::Right, Duration::ZERO)?;
    assert_eq!(a.history.cursors.len(), 2);
    a.dispatch(Action::Left, Duration::ZERO)?;
    a.dispatch(Action::Left, Duration::ZERO)?;
    assert_eq!(a.history.cursors.len(), 1);
    for expected in [Some(7), Some(30), None] {
        a.dispatch(Action::Period, Duration::ZERO)?;
        assert_eq!(a.history.filter.days, expected);
        assert_eq!(a.history.cursors.len(), 1);
    }
    a.dispatch(Action::Scope, Duration::ZERO)?;
    assert!(a.history.filter.exercise.is_some());
    assert!(a.history.filter.comparable());
    for expected in [1, 2, 0] {
        a.dispatch(Action::Graph, Duration::ZERO)?;
        assert_eq!(a.history.graph, expected);
    }
    a.dispatch(Action::NextField, Duration::ZERO)?;
    assert_eq!(a.history.scroll, 0);
    let mut words = App::new(Settings::default())?;
    words.history.enabled = true;
    words.dispatch(Action::History, Duration::ZERO)?;
    words.dispatch(Action::ToggleMode, Duration::ZERO)?;
    words.dispatch(Action::Scope, Duration::ZERO)?;
    assert_eq!(words.history.filter.language.as_deref(), Some("Rust"));
    Ok(())
}

#[test]
fn hap10_retry_signal_clear_and_global_controls_do_not_emit_extra_completion() -> TestResult {
    let mut a = app()?;
    a.history.enabled = true;
    finish(&mut a)?;
    let id = a.take_completion().ok_or("completion")?.id;
    a.history.save = SaveStatus::Unsaved;
    a.dispatch(Action::Text('r'), Duration::from_secs(1))?;
    assert!(a.history.retry);
    assert!(a.take_completion().is_none());
    a.dispatch(Action::History, Duration::from_secs(2))?;
    a.dispatch(Action::ClearHistory, Duration::from_secs(2))?;
    a.dispatch(Action::Resize(80, 24), Duration::from_secs(2))?;
    assert!(a.history.confirm_clear);
    a.dispatch(Action::Resize(1, 1), Duration::from_secs(2))?;
    a.dispatch(Action::Text('y'), Duration::from_secs(2))?;
    assert!(!a.history.clear_requested);
    a.dispatch(Action::Quit, Duration::from_secs(2))?;
    assert!(a.quit);
    assert!(a.take_completion().is_none());
    assert!(!id.is_empty());
    Ok(())
}

#[test]
fn hap10_cleared_results_cannot_be_resaved_by_retry() -> TestResult {
    use monkeytype_tui::history::{Filter, Store, service::Service};
    let directory =
        std::env::temp_dir().join(format!("monkeytype-service-test-{}", uuid::Uuid::new_v4()));
    let result = (|| -> TestResult {
        let mut a = app()?;
        let mut service = Service::new(true, Ok(Some(directory.clone())), &mut a);
        service.settle(&mut a)?;
        finish(&mut a)?;
        service.settle(&mut a)?;
        assert_eq!(a.history.save, SaveStatus::Saved);
        a.dispatch(Action::History, Duration::from_secs(1))?;
        service.settle(&mut a)?;
        a.dispatch(Action::ClearHistory, Duration::from_secs(1))?;
        a.dispatch(Action::Text('y'), Duration::from_secs(1))?;
        service.settle(&mut a)?;
        a.dispatch(Action::Cancel, Duration::from_secs(1))?;
        a.dispatch(Action::Text('r'), Duration::from_secs(1))?;
        service.settle(&mut a)?;
        assert_eq!(a.history.save, SaveStatus::Cleared);
        assert_eq!(
            Store::open(&directory)?.stats(&Filter::default(), 0)?.count,
            0
        );
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(directory);
    result
}

#[test]
fn his15_runtime_database_never_contains_the_typed_source() -> TestResult {
    use monkeytype_tui::{content::Exercise, history::service::Service};
    let directory =
        std::env::temp_dir().join(format!("monkeytype-privacy-test-{}", uuid::Uuid::new_v4()));
    let result = (|| -> TestResult {
        let marker = "sample_ONLY_LOCAL_AGGREGATE_42";
        let mut a = app()?;
        a.screen = Screen::Practice(Session::new(
            Prompt::new(marker, Mode::Code)?,
            TestDuration::Seconds30,
        ));
        a.exercise = Exercise::fixture();
        let mut service = Service::new(true, Ok(Some(directory.clone())), &mut a);
        for (i, c) in marker.chars().enumerate() {
            a.dispatch(Action::Text(c), Duration::from_millis(i as u64 * 10))?;
            service.settle(&mut a)?;
        }
        assert_eq!(a.history.save, SaveStatus::Saved);
        drop(service);
        let bytes = std::fs::read(directory.join("history.sqlite3"))?;
        assert!(!bytes.windows(marker.len()).any(|w| w == marker.as_bytes()));
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(directory);
    result
}
