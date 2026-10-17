//! Test-first acceptance checks. All filesystem state belongs to each test.
use monkeytype_tui::{
    content::{self, Exercise, Selector},
    engine::{FinishReason, Mode},
    history::{Counts, Filter, Record, Store, purge, resolve_directory},
};
use rand::{SeedableRng, rngs::SmallRng};
use std::{collections::HashSet, error::Error, fs, path::PathBuf};

type TestResult = Result<(), Box<dyn Error>>;
struct Temp(PathBuf);
impl Temp {
    fn new() -> Result<Self, Box<dyn Error>> {
        let path =
            std::env::temp_dir().join(format!("monkeytype-history-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
    fn dir(&self) -> PathBuf {
        self.0.join("state")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn record(index: i64) -> Record {
    Record::new(
        format!("00000000-0000-4000-8000-{index:012}"),
        index * 1000,
        Mode::Words,
        60,
        60_000_000_000,
        Counts {
            attempts: 100,
            correct_attempts: 90,
            current_correct: 80,
            backspaces: 10,
            uncorrected: 10,
        },
    )
}
fn filter() -> Filter {
    Filter {
        mode: Some(Mode::Words),
        seconds: Some(60),
        ..Filter::default()
    }
}

#[test]
fn his01_create_reopen_and_exact_roundtrip() -> TestResult {
    let t = Temp::new()?;
    let r = record(1);
    {
        let mut s = Store::open(&t.dir())?;
        assert!(s.save(&r)?);
    }
    let s = Store::open(&t.dir())?;
    assert_eq!(s.page(&filter(), None)?.rows, vec![r]);
    Ok(())
}

#[test]
fn his02_retry_is_idempotent_but_conflicting_id_is_rejected() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    let r = record(1);
    assert!(s.save(&r)?);
    assert!(!s.save(&r)?);
    let mut changed = r.clone();
    changed.completed_ms += 1;
    assert!(s.save(&changed).is_err());
    assert_eq!(s.stats(&filter(), 31 * 86_400_000)?.count, 1);
    Ok(())
}

#[test]
fn his03_invalid_counters_rates_duration_and_ids_never_enter_db() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    let mut cases = Vec::new();
    let mut r = record(1);
    r.attempts = 0;
    cases.push(r);
    let mut r = record(2);
    r.correct_attempts = 101;
    cases.push(r);
    let mut r = record(3);
    r.current_correct = 91;
    cases.push(r);
    let mut r = record(4);
    r.elapsed_ns = -1;
    cases.push(r);
    let mut r = record(5);
    r.seconds = 2;
    cases.push(r);
    let mut r = record(6);
    r.wpm = Some(f64::NAN);
    cases.push(r);
    let mut r = record(7);
    r.accuracy = Some(101.0);
    cases.push(r);
    let mut r = record(8);
    r.id = "bad id".into();
    cases.push(r);
    let mut r = record(9);
    r.elapsed_ns = 61_000_000_000;
    cases.push(r);
    for r in cases {
        assert!(s.save(&r).is_err(), "accepted {}", r.id);
    }
    assert_eq!(s.stats(&filter(), 0)?.count, 0);
    Ok(())
}

#[test]
fn his04_zero_elapsed_unavailable_metrics_stay_null() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    let mut r = record(1);
    r.elapsed_ns = 0;
    r.wpm = None;
    r.raw_wpm = None;
    r.accuracy = None;
    s.save(&r)?;
    assert_eq!(s.page(&filter(), None)?.rows[0].wpm, None);
    assert_eq!(s.stats(&filter(), 0)?.best, None);
    Ok(())
}

#[test]
fn his05_keyset_pages_have_no_gaps_with_equal_timestamps() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    for i in 1..=123 {
        let mut r = record(i);
        r.completed_ms = 100;
        s.save(&r)?;
    }
    let mut after = None;
    let mut ids = HashSet::new();
    let mut lengths = Vec::new();
    loop {
        let p = s.page(&filter(), after.as_ref())?;
        lengths.push(p.rows.len());
        for r in p.rows {
            assert!(ids.insert(r.id));
        }
        after = p.next;
        if after.is_none() {
            break;
        }
    }
    assert_eq!(lengths, vec![50, 50, 23]);
    assert_eq!(ids.len(), 123);
    Ok(())
}

#[test]
fn his06_filters_keep_modes_durations_versions_and_exercises_separate() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    s.save(&record(1))?;
    let mut r = record(2);
    r.mode = Mode::Code;
    r.reason = FinishReason::SnippetExhausted;
    r.exercise = Exercise::fixture();
    s.save(&r)?;
    let mut r = record(3);
    r.seconds = 120;
    s.save(&r)?;
    let mut r = record(4);
    r.metric_version = 2;
    s.save(&r)?;
    let mut r = record(5);
    r.exercise.pool_version = 2;
    s.save(&r)?;
    assert_eq!(s.page(&filter(), None)?.rows.len(), 1);
    let mut f = filter();
    f.mode = Some(Mode::Code);
    assert_eq!(s.page(&f, None)?.rows.len(), 1);
    assert!(!f.comparable());
    f.exercise = Some("fixture".into());
    f.exercise_version = Some(1);
    f.reason = Some(FinishReason::SnippetExhausted);
    assert!(f.comparable());
    let mut newer = Record::new(
        record(6).id,
        6000,
        Mode::Code,
        60,
        60_000_000_000,
        Counts {
            attempts: 100,
            correct_attempts: 90,
            current_correct: 80,
            backspaces: 10,
            uncorrected: 10,
        },
    );
    newer.exercise.version = 2;
    s.save(&newer)?;
    assert_eq!(s.page(&f, None)?.rows.len(), 1);
    f.exercise_version = Some(2);
    assert_eq!(s.page(&f, None)?.rows.len(), 1);
    Ok(())
}

#[test]
fn his07_weighted_accuracy_not_mean_percent_and_exact_median() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    let a = Record::new(
        record(1).id,
        1000,
        Mode::Words,
        60,
        60_000_000_000,
        Counts {
            attempts: 10,
            correct_attempts: 5,
            current_correct: 5,
            backspaces: 0,
            uncorrected: 5,
        },
    );
    let b = Record::new(
        record(2).id,
        2000,
        Mode::Words,
        60,
        60_000_000_000,
        Counts {
            attempts: 90,
            correct_attempts: 90,
            current_correct: 90,
            backspaces: 0,
            uncorrected: 0,
        },
    );
    s.save(&a)?;
    s.save(&b)?;
    let v = s.stats(&filter(), 30 * 86_400_000)?;
    assert_eq!(v.accuracy, Some(95.0));
    assert_eq!(v.best, Some(18.0));
    assert_eq!(v.median, Some(9.5));
    assert_eq!(v.practice_ns, 120_000_000_000.0);
    Ok(())
}

#[test]
fn his08_trends_are_bounded_chronological_and_rolling_needs_twenty() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    for i in 1..=19 {
        s.save(&record(i))?;
    }
    assert_eq!(s.stats(&filter(), 0)?.change, None);
    s.save(&record(20))?;
    assert_eq!(s.stats(&filter(), 0)?.change, Some(0.0));
    for i in 21..=80 {
        s.save(&record(i))?;
    }
    let points = s.stats(&filter(), 0)?.trend;
    assert_eq!(points.len(), 50);
    assert!(points.windows(2).all(|w| w[0].0 <= w[1].0));
    Ok(())
}

#[test]
fn his09_period_and_utc_activity_include_empty_days_and_boundaries() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    let day = 86_400_000;
    let now = 40 * day;
    for (i, stamp) in [(1, 9 * day), (2, 10 * day), (3, 39 * day), (4, 40 * day)] {
        let mut r = record(i);
        r.completed_ms = stamp;
        s.save(&r)?;
    }
    let mut f = filter();
    f.days = Some(30);
    f.now_ms = now;
    assert_eq!(s.page(&f, None)?.rows.len(), 2);
    let stats = s.stats(&f, now)?;
    assert_eq!(stats.daily.len(), 30);
    assert_eq!(stats.daily.iter().sum::<f64>(), 120.0);
    assert_eq!(stats.daily[0], 0.0);
    assert_eq!(stats.daily[29], 60.0);
    Ok(())
}

#[test]
fn his10_future_schema_and_corruption_are_preserved() -> TestResult {
    let t = Temp::new()?;
    {
        let _ = Store::open(&t.dir())?;
    }
    let db = t.dir().join("history.sqlite3");
    {
        let c = rusqlite::Connection::open(&db)?;
        c.pragma_update(None, "user_version", 999)?;
    }
    assert!(Store::open(&t.dir()).is_err());
    assert_eq!(
        rusqlite::Connection::open(&db)?
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
        999
    );
    fs::write(&db, b"not a database")?;
    assert!(Store::open(&t.dir()).is_err());
    assert_eq!(fs::read(&db)?, b"not a database");
    Ok(())
}

#[test]
fn his11_clear_is_transactional_and_resets_content_state() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    s.save(&record(1))?;
    let mut rng = SmallRng::seed_from_u64(5);
    s.reserve_code(&mut rng)?;
    s.clear()?;
    assert_eq!(s.stats(&filter(), 0)?.count, 0);
    assert!(s.page(&filter(), None)?.rows.is_empty());
    Ok(())
}

#[test]
fn his12_locked_write_fails_without_destroying_records_then_retry_works() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    s.save(&record(1))?;
    let other = rusqlite::Connection::open(t.dir().join("history.sqlite3"))?;
    other.execute_batch("BEGIN IMMEDIATE")?;
    let start = std::time::Instant::now();
    assert!(s.save(&record(2)).is_err());
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
    other.execute_batch("ROLLBACK")?;
    assert!(s.save(&record(2))?);
    assert_eq!(s.stats(&filter(), 0)?.count, 2);
    Ok(())
}

#[test]
fn his13_purge_refuses_live_instance_then_preserves_unrelated_file() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    s.save(&record(1))?;
    fs::write(t.dir().join("unrelated.txt"), b"keep")?;
    assert!(purge(&t.dir()).is_err());
    assert!(t.dir().join("history.sqlite3").exists());
    drop(s);
    purge(&t.dir())?;
    purge(&t.dir())?;
    assert!(!t.dir().join("history.sqlite3").exists());
    assert_eq!(fs::read(t.dir().join("unrelated.txt"))?, b"keep");
    Ok(())
}

#[cfg(unix)]
#[test]
fn his14_private_permissions_and_db_lock_directory_symlinks_rejected() -> TestResult {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let t = Temp::new()?;
    {
        let _ = Store::open(&t.dir())?;
    }
    for p in [
        t.dir(),
        t.dir().join("history.sqlite3"),
        t.dir().join("history.lock"),
    ] {
        assert_eq!(fs::metadata(p)?.permissions().mode() & 0o077, 0);
    }
    let db = t.dir().join("history.sqlite3");
    fs::remove_file(&db)?;
    let outside = t.0.join("outside");
    fs::write(&outside, b"keep")?;
    symlink(&outside, &db)?;
    assert!(Store::open(&t.dir()).is_err());
    assert!(purge(&t.dir()).is_err());
    assert_eq!(fs::read(&outside)?, b"keep");
    let linked = t.0.join("linked");
    symlink(t.dir(), &linked)?;
    assert!(Store::open(&linked).is_err());
    fs::remove_file(db)?;
    let lock = t.dir().join("history.lock");
    fs::remove_file(&lock)?;
    symlink(&outside, &lock)?;
    assert!(Store::open(&t.dir()).is_err());
    Ok(())
}

#[test]
fn his15_parameterized_metadata_is_data_not_sql_and_no_text_columns() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    let mut r = record(1);
    r.exercise.id = "exercise'); DROP TABLE sessions; --".into();
    s.save(&r)?;
    assert_eq!(s.stats(&filter(), 0)?.count, 1);
    let c = rusqlite::Connection::open(t.dir().join("history.sqlite3"))?;
    let sql: String = c.query_row(
        "SELECT sql FROM sqlite_schema WHERE name='sessions'",
        [],
        |r| r.get(0),
    )?;
    for forbidden in ["typed_text", "prompt_text", "keystroke", "source_text"] {
        assert!(!sql.contains(forbidden));
    }
    Ok(())
}

#[test]
fn his16_path_resolution_never_needs_environment_discovery() -> TestResult {
    assert_eq!(resolve_directory(false, None, None, None, false)?, None);
    assert_eq!(
        resolve_directory(false, Some("relative".into()), None, None, false)?,
        None
    );
    assert_eq!(
        resolve_directory(true, Some("/tmp/custom".into()), None, None, false)?,
        Some("/tmp/custom".into())
    );
    assert_eq!(
        resolve_directory(
            true,
            None,
            Some("/home/test".into()),
            Some("/state".into()),
            false
        )?,
        Some("/state/monkeytype-tui".into())
    );
    assert_eq!(
        resolve_directory(
            true,
            None,
            Some("/home/test".into()),
            Some("relative".into()),
            false
        )?,
        Some("/home/test/.local/state/monkeytype-tui".into())
    );
    assert_eq!(
        resolve_directory(true, None, Some("/home/test".into()), None, true)?,
        Some("/home/test/Library/Application Support/monkeytype-tui".into())
    );
    assert!(resolve_directory(true, Some("relative".into()), None, None, false).is_err());
    Ok(())
}

#[test]
fn cnt01_catalog_is_large_original_ascii_unique_and_valid() -> TestResult {
    let words = content::words();
    assert!(words.len() >= 1000);
    assert!(
        words
            .iter()
            .all(|word| !word.is_empty() && word.bytes().all(|b| b.is_ascii_lowercase()))
    );
    assert_eq!(words.len(), words.iter().collect::<HashSet<_>>().len());
    let code = content::catalog();
    assert!(code.len() >= 60);
    assert_eq!(
        code.len(),
        code.iter()
            .map(|e| &e.meta.id)
            .collect::<HashSet<_>>()
            .len()
    );
    for language in ["Rust", "Python", "JavaScript"] {
        assert!(
            code.iter()
                .filter(|e| e.meta.language.as_deref() == Some(language))
                .count()
                >= 20
        );
    }
    for e in code {
        let prompt = monkeytype_tui::engine::prompt::Prompt::new(&e.source, Mode::Code)?;
        assert!(
            prompt.expected().len() <= 512,
            "bundled snippets must remain short"
        );
    }
    Ok(())
}

#[test]
fn cnt02_seeded_word_selection_replenishes_without_adjacent_duplicates() -> TestResult {
    let mut a = Selector::seeded(42);
    let mut b = Selector::seeded(42);
    let mut previous = String::new();
    let mut seen = HashSet::new();
    for _ in 0..100 {
        let chunk = a.words();
        assert_eq!(chunk, b.words());
        assert_eq!(chunk.split_whitespace().count(), 32);
        for word in chunk.split_whitespace() {
            assert_ne!(word, previous);
            previous = word.into();
            seen.insert(word.to_owned());
        }
    }
    assert!(seen.len() >= 1000);
    Ok(())
}

#[test]
fn cnt03_memory_code_deck_covers_catalog_and_never_repeats_boundary() -> TestResult {
    let mut a = Selector::seeded(7);
    let mut seen = HashSet::new();
    let n = content::catalog().len();
    let mut last = String::new();
    for _ in 0..n {
        let e = a.code();
        assert_ne!(e.meta.id, last);
        last = e.meta.id.clone();
        assert!(seen.insert(e.meta.id));
    }
    assert_ne!(a.code().meta.id, last);
    Ok(())
}

#[test]
fn cnt04_persisted_deck_survives_restart_and_two_instances() -> TestResult {
    let t = Temp::new()?;
    let mut rng = SmallRng::seed_from_u64(11);
    let mut seen = HashSet::new();
    for _ in 0..content::catalog().len() {
        let mut s = Store::open(&t.dir())?;
        assert!(seen.insert(s.reserve_code(&mut rng)?.meta.id));
    }
    let mut a = Store::open(&t.dir())?;
    let mut b = Store::open(&t.dir())?;
    assert_ne!(
        a.reserve_code(&mut rng)?.meta.id,
        b.reserve_code(&mut rng)?.meta.id
    );
    Ok(())
}

#[test]
fn cnt05_obsolete_or_corrupt_catalog_state_is_rebuilt() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    let mut rng = SmallRng::seed_from_u64(9);
    s.reserve_code(&mut rng)?;
    let c = rusqlite::Connection::open(t.dir().join("history.sqlite3"))?;
    c.execute("UPDATE content_selection SET exercise_id='removed-exercise' WHERE position=(SELECT min(position) FROM content_selection)",[])?;
    let e = s.reserve_code(&mut rng)?;
    assert!(content::catalog().iter().any(|v| v.meta.id == e.meta.id));
    c.execute("UPDATE content_meta SET version=0", [])?;
    s.reserve_code(&mut rng)?;
    assert_eq!(
        c.query_row("SELECT version FROM content_meta", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    Ok(())
}

#[test]
fn his17_unrelated_schema_readonly_file_and_invalid_loaded_row_are_preserved() -> TestResult {
    let t = Temp::new()?;
    let mut s = Store::open(&t.dir())?;
    s.save(&record(1))?;
    drop(s);
    let db = t.dir().join("history.sqlite3");
    let c = rusqlite::Connection::open(&db)?;
    c.execute_batch(
        "PRAGMA ignore_check_constraints=ON; UPDATE sessions SET current_correct=999;",
    )?;
    drop(c);
    let s = Store::open(&t.dir())?;
    assert!(s.page(&filter(), None).is_err());
    drop(s);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&db, fs::Permissions::from_mode(0o400))?;
        assert!(Store::open(&t.dir()).is_err());
        fs::set_permissions(&db, fs::Permissions::from_mode(0o600))?;
    }
    let d = t.0.join("unrelated-db");
    {
        let _ = Store::open(&d)?;
    }
    let c = rusqlite::Connection::open(d.join("history.sqlite3"))?;
    c.execute_batch("PRAGMA user_version=0;")?;
    assert!(Store::open(&d).is_err());
    assert_eq!(
        c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn his18_failed_clear_keeps_results_and_deck_and_purge_missing_creates_nothing() -> TestResult {
    let t = Temp::new()?;
    let never = t.0.join("never-created");
    purge(&never)?;
    assert!(!never.exists());
    let mut s = Store::open(&t.dir())?;
    s.save(&record(1))?;
    s.reserve_code(&mut SmallRng::seed_from_u64(8))?;
    let c = rusqlite::Connection::open(t.dir().join("history.sqlite3"))?;
    c.execute_batch("BEGIN IMMEDIATE")?;
    assert!(s.clear().is_err());
    c.execute_batch("ROLLBACK")?;
    assert_eq!(s.stats(&filter(), 0)?.count, 1);
    assert_eq!(
        c.query_row("SELECT count(*) FROM content_selection", [], |r| r
            .get::<_, usize>(0))?,
        59
    );
    drop(c);
    drop(s);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(t.dir(), fs::Permissions::from_mode(0o500))?;
        let outcome = purge(&t.dir());
        fs::set_permissions(t.dir(), fs::Permissions::from_mode(0o700))?;
        assert!(outcome.is_err());
        assert!(t.dir().join("history.sqlite3").exists());
    }
    Ok(())
}

#[test]
fn cnt06_single_entry_deck_and_entropy_error_are_handled() -> TestResult {
    let mut selector = Selector::seeded(1);
    let mut deck = Vec::new();
    for _ in 0..3 {
        assert_eq!(
            content::draw_index(&mut deck, Some(0), 1, &mut selector.rng),
            Some(0)
        );
    }
    assert_eq!(
        content::draw_index(&mut Vec::new(), None, 0, &mut selector.rng),
        None
    );
    let bad: Result<[u8; 32], std::io::Error> =
        Err(std::io::Error::other("fixture entropy failure"));
    assert!(Selector::from_entropy(bad).is_err());
    Ok(())
}

#[test]
fn his06_invalid_filter_values_are_rejected_without_queries_or_panics() -> TestResult {
    let t = Temp::new()?;
    let s = Store::open(&t.dir())?;
    let mut f = filter();
    f.days = Some(i64::MIN);
    assert!(s.page(&f, None).is_err());
    f.days = None;
    f.seconds = Some(u64::MAX);
    assert!(s.stats(&f, 0).is_err());
    Ok(())
}
