use crate::{
    app::{App, MIN_HEIGHT, MIN_WIDTH, Screen},
    engine::{Mode, Phase, Report, Session, Settings},
    history::{HistoryTab, date_label},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Axis, Bar, BarChart, BarGroup, Block, Borders, Chart, Clear, Dataset, GraphType, Paragraph,
        Sparkline, Wrap,
    },
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
    if matches!(app.screen, Screen::History { .. }) {
        history_view(frame, app, inner);
        return;
    }
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
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("monkeytype-tui", strong()),
                Span::raw(format!(
                    "    {} / {}s",
                    settings.mode,
                    settings.duration.seconds()
                )),
            ]),
            Line::styled(
                if settings.mode == Mode::Code {
                    format!("Exercise: {}", app.exercise.id)
                } else {
                    "Pool: English common / v1".into()
                },
                dim(),
            ),
        ]),
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
                "^C quit  F2 mode  F3 time  F4 history  Esc setup"
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
                "^C quit  Enter restart  F4 history  Esc setup",
                app.notice.unwrap_or(app.history.save.label()),
            );
        }
        Screen::History { .. } | Screen::Transition => {}
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

fn history_view(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let h = &app.history;
    let regions = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(2),
        Constraint::Length(3),
    ])
    .split(area);
    let scope = h
        .filter
        .exercise
        .as_deref()
        .or(h.filter.language.as_deref())
        .unwrap_or(if h.filter.mode == Some(Mode::Code) {
            "mixed snippets"
        } else {
            "English common"
        });
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                format!(
                    "{}  {} / {}s",
                    if h.tab == HistoryTab::Recent {
                        "HISTORY"
                    } else {
                        "PROGRESS"
                    },
                    h.filter.mode.unwrap_or(Mode::Words),
                    h.filter.seconds.unwrap_or(60)
                ),
                strong(),
            ),
            Line::raw(format!(
                "{scope} / {} / metrics v{}",
                h.filter
                    .days
                    .map_or_else(|| "all time".into(), |d| format!("{d} UTC days")),
                h.filter.metric_version
            )),
        ]),
        regions[0],
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(format!(
                "Completed: {}  Practice: {:.1}m  Best: {}",
                h.stats.count,
                h.stats.practice_ns / 60_000_000_000.0,
                rate(h.stats.best)
            )),
            Line::raw(format!(
                "Median WPM: {}  Weighted accuracy: {}%",
                rate(h.stats.median),
                rate(h.stats.accuracy)
            )),
        ]),
        regions[1],
    );
    if h.unavailable {
        frame.render_widget(Paragraph::new("History unavailable. Your database is preserved.\nPractice and exit remain available.").wrap(Wrap{trim:false}),regions[2]);
    } else if h.tab == HistoryTab::Recent {
        let mut lines = vec![Line::styled(
            "UTC completion     Exercise        Limit/actual End WPM/raw Acc% Errors",
            dim(),
        )];
        for r in h
            .page
            .rows
            .iter()
            .skip(h.scroll)
            .take(regions[2].height.saturating_sub(1) as usize)
        {
            lines.push(Line::raw(format!(
                "{} {:<15} {}/{:.1}s {} {}/{} {} {}",
                date_label(r.completed_ms),
                r.exercise.id,
                r.seconds,
                r.elapsed_ns as f64 / 1e9,
                if r.reason == crate::engine::FinishReason::Timeout {
                    "time"
                } else {
                    "done"
                },
                rate(r.wpm),
                rate(r.raw_wpm),
                rate(r.accuracy),
                r.attempts - r.correct_attempts
            )));
        }
        if h.page.rows.is_empty() {
            lines.push(Line::raw("No completed tests in these filters."));
        }
        frame.render_widget(Paragraph::new(lines), regions[2]);
    } else {
        progress(frame, app, regions[2]);
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw("^C quit  Esc back  F4 recent/progress  Up/Down rows  Left/Right page"),
            Line::raw("F2 mode  F3 duration  F5 period  F6 code scope"),
            Line::raw(
                app.notice
                    .unwrap_or("F7 speed/accuracy/activity  F8 clear history"),
            ),
        ]),
        regions[3],
    );
    if h.confirm_clear {
        let width = area.width.min(64);
        let height = area.height.min(6);
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        frame.render_widget(Paragraph::new("Delete all saved results and reset the code deck?\n\ny: clear ALL history    Enter / Esc / n: cancel").wrap(Wrap{trim:false}).block(Block::default().borders(Borders::ALL).title(" CLEAR HISTORY ")),popup);
    }
}

fn progress(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let h = &app.history;
    let regions = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let graph = regions[0];
    let message = if !h.filter.comparable() {
        "Mixed snippets: no equivalent-exercise PB or pattern.".into()
    } else {
        h.stats.change.map_or_else(
            || "20 matching tests needed for a ten-vs-ten speed pattern.".into(),
            |v| format!("Latest 10 vs previous 10: {v:+.1} WPM"),
        )
    };
    frame.render_widget(Paragraph::new(message).style(dim()), regions[1]);
    if h.graph == 2 {
        let labels: Vec<_> = (1..=h.stats.daily.len())
            .map(|i| {
                if i % 5 == 0 {
                    i.to_string()
                } else {
                    String::new()
                }
            })
            .collect();
        let bars: Vec<_> = h
            .stats
            .daily
            .iter()
            .enumerate()
            .map(|(i, seconds)| {
                Bar::default()
                    .value((seconds / 6.0).round() as u64)
                    .text_value(format!("{:.1}", seconds / 60.0))
                    .label(Line::raw(labels[i].clone()))
            })
            .collect();
        frame.render_widget(
            BarChart::default()
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Practice minutes / UTC day (oldest to today) "),
                )
                .data(BarGroup::default().bars(&bars))
                .bar_width(if graph.width >= 70 { 2 } else { 1 })
                .bar_gap(0)
                .bar_style(strong())
                .value_style(Style::default()),
            graph,
        );
        return;
    }
    let accuracy = h.graph == 1;
    let points: Vec<_> = h
        .stats
        .trend
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let y = if accuracy { p.2 } else { p.1 };
            y.is_finite().then_some((i as f64, y))
        })
        .collect();
    let title = if accuracy {
        " Accuracy (%) / recent sessions "
    } else {
        " WPM / recent sessions "
    };
    if points.is_empty() {
        frame.render_widget(
            Paragraph::new("No measured tests in these filters.")
                .block(Block::default().borders(Borders::ALL).title(title)),
            graph,
        );
        return;
    }
    if graph.height < 6 || graph.width < 50 {
        let values: Vec<_> = points.iter().map(|p| (p.1 * 10.0).round() as u64).collect();
        frame.render_widget(
            Sparkline::default()
                .block(Block::default().borders(Borders::TOP).title(title))
                .data(&values)
                .style(strong()),
            graph,
        );
        return;
    }
    let average: Vec<_> = points
        .windows(10)
        .map(|p| (p[9].0, p.iter().map(|p| p.1).sum::<f64>() / 10.0))
        .collect();
    let datasets = vec![
        Dataset::default()
            .name(if accuracy { "Accuracy" } else { "WPM" })
            .marker(ratatui::symbols::Marker::Dot)
            .graph_type(GraphType::Line)
            .style(strong())
            .data(&points),
        Dataset::default()
            .name("10-test mean")
            .marker(ratatui::symbols::Marker::Dot)
            .graph_type(GraphType::Line)
            .style(dim())
            .data(&average),
    ];
    let ymax = if accuracy {
        100.0
    } else {
        points.iter().map(|p| p.1).fold(1.0, f64::max) * 1.1
    };
    frame.render_widget(
        Chart::new(datasets)
            .block(Block::default().borders(Borders::ALL).title(title))
            .x_axis(
                Axis::default()
                    .bounds([0.0, (h.stats.trend.len().max(2) - 1) as f64])
                    .labels(["older", "newer"]),
            )
            .y_axis(
                Axis::default()
                    .bounds([0.0, ymax])
                    .labels(["0".to_owned(), format!("{ymax:.0}")]),
            ),
        graph,
    );
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
