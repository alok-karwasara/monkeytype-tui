use crate::{
    app::{App, MIN_HEIGHT, MIN_WIDTH, Screen},
    engine::{Mode, Phase, Report, Session, Settings},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::time::Duration;

fn strong() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}
fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}
fn rate(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |v| format!("{v:.1}"))
}

pub fn draw(frame: &mut Frame<'_>, app: &App, now: Duration) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().fg(Color::Reset).bg(Color::Reset)),
        area,
    );
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(Paragraph::new(format!("Resize to {MIN_WIDTH}×{MIN_HEIGHT}\nCtrl+C quits\nTimer continues during an active test.")).wrap(Wrap { trim: false }), area);
        return;
    }
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    let regions = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Min(2),
        Constraint::Length(2),
    ])
    .split(inner);
    let settings = match &app.screen {
        Screen::Settings { draft, .. } => *draft,
        _ => app.settings,
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("monkeytype-tui", strong()),
            Span::raw(format!(
                "    {} / {}s",
                settings.mode,
                settings.duration.seconds()
            )),
        ])),
        regions[0],
    );
    match &app.screen {
        Screen::Practice(session) => {
            let metrics = session.metrics(now);
            let remaining = session.remaining(now).as_secs_f64().ceil() as u64;
            instruments(
                frame,
                regions[1],
                [
                    format!("{remaining}s"),
                    rate(metrics.wpm),
                    metrics
                        .accuracy
                        .map_or_else(|| "—".to_owned(), |a| format!("{a:.1}%")),
                    session.counters().errors().to_string(),
                ],
            );
            let title = if session.phase() == Phase::Ready {
                " READY — first character starts "
            } else {
                " TYPING "
            };
            let block = Block::default()
                .borders(Borders::TOP | Borders::BOTTOM)
                .title(title);
            let prompt_area = block.inner(regions[2]);
            frame.render_widget(block, regions[2]);
            prompt(frame, prompt_area, session);
            let ready = session.phase() == Phase::Ready;
            let line1 = if ready {
                "F2 mode  F3 time  Esc setup  ^C quit"
            } else {
                "BS correct  Esc abandon  ^C quit"
            };
            let explanation = if settings.mode == Mode::Code {
                "Line edges/newlines skipped; internal spaces count."
            } else {
                "Type words and spaces. Keep going until time runs out."
            };
            footer(frame, regions[3], line1, app.notice.unwrap_or(explanation));
        }
        Screen::Settings { draft, field, .. } => {
            frame.render_widget(Paragraph::new("SETTINGS").style(strong()), regions[1]);
            settings_view(frame, regions[2], *draft, *field);
            footer(
                frame,
                regions[3],
                "↑↓/Tab field  ←→ change  Enter apply",
                app.notice
                    .unwrap_or("Esc cancel  F2 mode  F3 time  ^C quit"),
            );
        }
        Screen::Results(report) => {
            instruments(
                frame,
                regions[1],
                [
                    format!("{:.1}s", report.elapsed.as_secs_f64()),
                    rate(report.metrics.wpm),
                    report
                        .metrics
                        .accuracy
                        .map_or_else(|| "—".to_owned(), |a| format!("{a:.1}%")),
                    report.counters.errors().to_string(),
                ],
            );
            results(frame, regions[2], report);
            footer(
                frame,
                regions[3],
                "Enter restart  Esc setup  ^C quit",
                app.notice
                    .unwrap_or("History is planned; results are not saved."),
            );
        }
    }
    if app.abandon {
        let width = area.width.saturating_sub(4).min(62);
        let height = 7.min(area.height);
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        frame.render_widget(Paragraph::new("Abandon this test?\n\nEnter / y: abandon    Esc / n: keep typing\nThe timer continues while this prompt is open.").wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title(" CONFIRM ABANDON ")), popup);
    }
}

fn instruments(frame: &mut Frame<'_>, area: Rect, values: [String; 4]) {
    let columns = Layout::horizontal([Constraint::Ratio(1, 4); 4]).split(area);
    for ((label, value), column) in ["TIME", "WPM", "ACCURACY", "ERRORS"]
        .into_iter()
        .zip(values)
        .zip(columns.iter())
    {
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(label, dim()),
                Line::styled(value, strong()),
            ]),
            *column,
        );
    }
}

fn footer(frame: &mut Frame<'_>, area: Rect, first: &str, second: &str) {
    // Each line may be clipped on narrow terminals; full settings remain available via Esc.
    frame.render_widget(
        Paragraph::new(vec![Line::raw(first), Line::styled(second, dim())]),
        area,
    );
}

fn settings_view(frame: &mut Frame<'_>, area: Rect, settings: Settings, field: usize) {
    let selected = strong().add_modifier(Modifier::UNDERLINED);
    let lines = vec![
        Line::styled(
            format!(
                "{} Mode       {}",
                if field == 0 { ">" } else { " " },
                settings.mode
            ),
            if field == 0 {
                selected
            } else {
                Style::default()
            },
        ),
        Line::styled(
            format!(
                "{} Duration   {} seconds",
                if field == 1 { ">" } else { " " },
                settings.duration.seconds()
            ),
            if field == 1 {
                selected
            } else {
                Style::default()
            },
        ),
        Line::raw(""),
        Line::raw("Appearance follows your terminal theme and font."),
        Line::raw("Code ends at the time limit or snippet completion."),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn results(frame: &mut Frame<'_>, area: Rect, report: &Report) {
    let lines = vec![
        Line::styled("TEST COMPLETE", strong()),
        Line::raw(format!("Reason: {}", report.reason)),
        Line::raw(format!(
            "Raw WPM: {}   Uncorrected: {}",
            rate(report.metrics.raw_wpm),
            report.uncorrected
        )),
        Line::raw(format!(
            "Attempts: {}   Correct attempts: {}",
            report.counters.attempts, report.counters.correct_attempts
        )),
        Line::raw(""),
        Line::styled(
            "Accuracy retains corrected mistakes; WPM counts correct progress.",
            dim(),
        ),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn prompt(frame: &mut Frame<'_>, area: Rect, session: &Session) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = usize::from(area.width);
    let height = usize::from(area.height);
    let source = session.prompt();
    let cursor = source
        .expected()
        .get(session.cursor())
        .or_else(|| source.expected().last());
    let Some(cursor) = cursor else {
        return;
    };
    let wrapped_rows = |length: usize| length.div_ceil(width).max(1);
    let before_caret: usize = source
        .lines()
        .iter()
        .take(cursor.row)
        .map(|r| wrapped_rows(r.len()))
        .sum();
    let total: usize = source.lines().iter().map(|r| wrapped_rows(r.len())).sum();
    let cursor_row = before_caret + cursor.column / width;
    let top = cursor_row
        .saturating_sub(height / 2)
        .min(total.saturating_sub(height));
    let mut logical_top = 0;
    let mut lines = Vec::new();
    for range in source.lines() {
        let count = wrapped_rows(range.len());
        let begin = top.saturating_sub(logical_top).min(count);
        let end = (top + height).saturating_sub(logical_top).min(count);
        for row in begin..end {
            let start_offset = range.start + row * width;
            let end_offset = range.end.min(start_offset + width);
            let mut spans = Vec::new();
            for offset in start_offset..end_offset {
                let original = source.display().as_bytes()[offset];
                let mut character = char::from(original);
                let mut style = dim();
                if let Some(index) = source.scored_at(offset) {
                    if let Some(&typed) = session.typed().get(index) {
                        if typed == character {
                            style = strong();
                        } else {
                            character = if typed == ' ' { '_' } else { typed };
                            style = Style::default()
                                .fg(Color::Red)
                                .add_modifier(Modifier::UNDERLINED);
                        }
                    }
                    if index == session.cursor() {
                        style = style.add_modifier(Modifier::REVERSED);
                    }
                }
                spans.push(Span::styled(character.to_string(), style));
            }
            lines.push(Line::from(spans));
        }
        logical_top += count;
        if logical_top >= top + height {
            break;
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}
