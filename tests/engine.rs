use monkeytype_tui::engine::{
    EngineInput, FinishReason, Mode, Phase, Session, TestDuration,
    metrics::{Counters, Metrics},
    prompt::{Prompt, PromptError},
};
use serde_json::Value;
use std::{error::Error, io, time::Duration};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
fn fixtures() -> TestResult<Value> {
    Ok(serde_json::from_str(include_str!(
        "fixtures/typing-cases.json"
    ))?)
}
fn text(v: &Value) -> TestResult<&str> {
    v.as_str()
        .ok_or_else(|| io::Error::other("expected fixture string").into())
}
fn number(v: &Value) -> TestResult<u64> {
    v.as_u64()
        .ok_or_else(|| io::Error::other("expected fixture number").into())
}
fn mode(v: &Value) -> TestResult<Mode> {
    match text(v)? {
        "words" => Ok(Mode::Words),
        "code" => Ok(Mode::Code),
        _ => Err("invalid fixture mode".into()),
    }
}
fn array(v: &Value) -> TestResult<&Vec<Value>> {
    v.as_array()
        .ok_or_else(|| io::Error::other("expected fixture array").into())
}

#[test]
fn p01_to_p14_prompt_fixtures() -> TestResult {
    for case in array(&fixtures()?["prompt_cases"])? {
        let result = Prompt::new(text(&case["source"])?, mode(&case["mode"])?);
        if let Some(error) = case["expected_error"].as_str() {
            let expected = match error {
                "empty_scored_prompt" => PromptError::Empty,
                "unsupported_control_character" => PromptError::UnsupportedControl,
                "unsupported_character" => PromptError::UnsupportedCharacter,
                _ => return Err("unknown fixture error".into()),
            };
            assert_eq!(result.err(), Some(expected), "{case}");
            continue;
        }
        let prompt = result?;
        let required: String = prompt.expected().iter().map(|u| u.character).collect();
        assert_eq!(required, text(&case["expected_required"])?, "{case}");
        if let Some(display) = case["expected_normalized_display"].as_str() {
            assert_eq!(prompt.display(), display, "{case}");
        }
        for (index, unit) in prompt.expected().iter().enumerate() {
            assert_eq!(
                prompt.display().as_bytes()[unit.display_offset],
                unit.character as u8
            );
            assert_eq!(prompt.scored_at(unit.display_offset), Some(index));
        }
        if let Some(positions) = case["selected_positions"].as_array() {
            for position in positions {
                let unit = prompt
                    .expected()
                    .get(number(&position["scored_index"])? as usize)
                    .ok_or("missing expected position")?;
                assert_eq!(unit.row as u64, number(&position["row"])?);
                assert_eq!(unit.column as u64, number(&position["column"])?);
            }
        }
    }
    Ok(())
}

fn execute_trace(case: &Value) -> TestResult {
    let prompt = Prompt::new(text(&case["source"])?, mode(&case["mode"])?)?;
    let duration =
        TestDuration::from_seconds(number(&case["limit_ms"])? / 1000).ok_or("invalid duration")?;
    let mut session = Session::new(prompt, duration);
    let mut now = Duration::ZERO;
    for event in array(&case["events"])? {
        now = Duration::from_millis(number(&event["at_ms"])?);
        match text(&event["action"])? {
            "type" => {
                let c = text(&event["character"])?
                    .chars()
                    .next()
                    .ok_or("empty fixture character")?;
                session.apply(EngineInput::Character(c), now);
            }
            "backspace" => {
                session.apply(EngineInput::Backspace, now);
            }
            "advance_time" => session.advance_time(now),
            _ => return Err("unknown trace event".into()),
        }
    }
    let expected = &case["expected"];
    let phase = match session.phase() {
        Phase::Ready => "ready",
        Phase::Running { .. } => "running",
        Phase::Finished { .. } => "finished",
    };
    assert_eq!(phase, text(&expected["phase"])?, "{case}");
    assert_eq!(
        session.typed().iter().collect::<String>(),
        text(&expected["typed_prefix"])?,
        "{case}"
    );
    assert_eq!(
        session.cursor() as u64,
        number(&expected["cursor"])?,
        "{case}"
    );
    let counters = session.counters();
    assert_eq!(counters.attempts, number(&expected["attempts"])?, "{case}");
    assert_eq!(
        counters.correct_attempts,
        number(&expected["correct_attempts"])?,
        "{case}"
    );
    assert_eq!(
        counters.current_correct as u64,
        number(&expected["current_correct"])?,
        "{case}"
    );
    assert_eq!(
        session.uncorrected() as u64,
        number(&expected["uncorrected"])?,
        "{case}"
    );
    assert_eq!(counters.errors(), number(&expected["errors"])?, "{case}");
    assert_eq!(
        session.elapsed(now).map(|d| d.as_millis() as u64),
        expected["elapsed_ms"].as_u64(),
        "{case}"
    );
    let reason = session.report().map(|r| match r.reason {
        FinishReason::Timeout => "timeout",
        FinishReason::SnippetExhausted => "snippet_exhausted",
    });
    assert_eq!(reason, expected["finish_reason"].as_str(), "{case}");
    if !case["expected_next_display_position"].is_null() {
        let next = session
            .prompt()
            .expected()
            .get(session.cursor())
            .ok_or("missing caret")?;
        assert_eq!(
            next.row as u64,
            number(&case["expected_next_display_position"]["row"])?
        );
        assert_eq!(
            next.column as u64,
            number(&case["expected_next_display_position"]["column"])?
        );
    }
    Ok(())
}

#[test]
fn eng01_to_eng11_session_traces() -> TestResult {
    for case in array(&fixtures()?["session_cases"])? {
        execute_trace(case)?;
    }
    Ok(())
}

#[test]
fn time03_to_time11_all_duration_boundaries() -> TestResult {
    for case in array(&fixtures()?["timing_cases"])? {
        execute_trace(case)?;
    }
    Ok(())
}

#[test]
fn met01_to_met06_metric_fixtures() -> TestResult {
    for case in array(&fixtures()?["metric_cases"])? {
        let c = &case["counters"];
        let counters = Counters {
            attempts: number(&c["attempts"])?,
            correct_attempts: number(&c["correct_attempts"])?,
            current_correct: number(&c["current_correct"])? as usize,
            backspaces: 0,
        };
        let metrics = Metrics::calculate(
            counters,
            case["elapsed_ms"].as_u64().map(Duration::from_millis),
        );
        for (actual, key) in [
            (metrics.accuracy, "accuracy_percent"),
            (metrics.raw_wpm, "raw_wpm"),
            (metrics.wpm, "wpm"),
        ] {
            match (actual, case["expected"][key].as_f64()) {
                (Some(a), Some(e)) => assert!((a - e).abs() < 1e-9, "{case}: {key}: {a} != {e}"),
                (None, None) => {}
                _ => panic!("metric presence mismatch: {case}: {key}"),
            }
        }
        assert_eq!(counters.errors(), number(&case["expected"]["errors"])?);
    }
    Ok(())
}

#[test]
fn time01_time02_time09_ignored_input_and_start_offset() -> TestResult {
    let mut s = Session::new(Prompt::new("abc", Mode::Code)?, TestDuration::Seconds30);
    for c in ['\n', '\t', '\0', 'é'] {
        s.apply(EngineInput::Character(c), Duration::from_secs(20));
    }
    assert_eq!(s.phase(), Phase::Ready);
    s.apply(EngineInput::Character('x'), Duration::from_secs(25));
    assert_eq!(s.counters().attempts, 1);
    assert_eq!(s.counters().errors(), 1);
    assert_eq!(
        s.elapsed(Duration::from_millis(25_750)),
        Some(Duration::from_millis(750))
    );
    Ok(())
}

#[test]
fn eng12_word_replenishment_preserves_prefix_and_separator() -> TestResult {
    let mut s = Session::new(
        Prompt::new("red blue", Mode::Words)?,
        TestDuration::Seconds30,
    );
    for c in "red blue".chars() {
        s.apply(EngineInput::Character(c), Duration::ZERO);
    }
    assert!(matches!(s.phase(), Phase::Running { .. }));
    s.append_words("green gold")?;
    assert_eq!(s.prompt().display(), "red blue green gold");
    assert_eq!(s.prompt().expected()[s.cursor()].character, ' ');
    assert_eq!(s.typed().iter().collect::<String>(), "red blue");
    assert_eq!(s.counters().errors(), 0);
    Ok(())
}

#[test]
fn met07_formatting_does_not_change_scoring() -> TestResult {
    let mut sessions = [
        Session::new(Prompt::new("a b", Mode::Code)?, TestDuration::Seconds30),
        Session::new(
            Prompt::new("    a b   \n", Mode::Code)?,
            TestDuration::Seconds30,
        ),
    ];
    for s in &mut sessions {
        for (i, c) in "a b".chars().enumerate() {
            s.apply(EngineInput::Character(c), Duration::from_secs(i as u64));
        }
    }
    assert_eq!(sessions[0].report(), sessions[1].report());
    Ok(())
}

#[test]
fn generated_action_sequences_preserve_invariants() -> TestResult {
    for seed in 0..32_u64 {
        let mut random = seed + 1;
        let mut s = Session::new(
            Prompt::new("a b;\n    c:d;", Mode::Code)?,
            TestDuration::Seconds30,
        );
        let mut frozen = None;
        for index in 0..256 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let action = match random % 5 {
                0 => EngineInput::Backspace,
                1 => EngineInput::Character(' '),
                2 => EngineInput::Character('a'),
                3 => EngineInput::Character(';'),
                _ => EngineInput::Character('x'),
            };
            s.apply(action, Duration::from_millis(index * 200));
            let c = s.counters();
            assert!(s.cursor() <= s.prompt().expected().len());
            assert_eq!(c.current_correct + s.uncorrected(), s.cursor());
            assert!(
                c.correct_attempts <= c.attempts && c.current_correct as u64 <= c.correct_attempts
            );
            let actual_correct = s
                .typed()
                .iter()
                .zip(s.prompt().expected())
                .filter(|(typed, expected)| **typed == expected.character)
                .count();
            assert_eq!(actual_correct, c.current_correct);
            if let Some(report) = &frozen {
                assert_eq!(s.report().as_ref(), Some(report));
            } else {
                frozen = s.report();
            }
        }
    }
    Ok(())
}

#[test]
fn oversized_and_invalid_word_prompts_fail_before_timing() {
    assert_eq!(
        Prompt::new(&"x".repeat(1_048_577), Mode::Code).err(),
        Some(PromptError::TooLarge)
    );
    assert_eq!(
        Prompt::new("red\nblue", Mode::Words).err(),
        Some(PromptError::InvalidWordContent)
    );
}
