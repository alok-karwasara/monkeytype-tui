//! Persistence runs only at the runtime boundary, outside active typing.
use super::{HistoryResult, Page, Record, SaveStatus, Stats, Store, now_ms};
use crate::{
    app::{App, Screen},
    engine::Phase,
};
use std::path::PathBuf;

pub struct Service {
    enabled: bool,
    directory: Option<PathBuf>,
    store: Option<Store>,
    pending: Option<Record>,
}
impl Service {
    pub fn new(enabled: bool, directory: HistoryResult<Option<PathBuf>>, app: &mut App) -> Self {
        app.history.enabled = enabled;
        app.history.save = if enabled {
            SaveStatus::Pending
        } else {
            SaveStatus::Disabled
        };
        let directory = if enabled {
            directory.ok().flatten()
        } else {
            None
        };
        let store = directory.as_ref().and_then(|p| Store::open(p).ok());
        app.history.unavailable = enabled && store.is_none();
        if app.history.unavailable {
            app.notice = Some("History unavailable; practice still works.");
        }
        Self {
            enabled,
            directory,
            store,
            pending: None,
        }
    }
    fn save(&mut self, app: &mut App) {
        if !self.enabled || self.pending.is_none() {
            return;
        }
        if self.store.is_none() {
            self.store = self.directory.as_ref().and_then(|p| Store::open(p).ok());
        }
        let saved = match (&mut self.store, &self.pending) {
            (Some(store), Some(record)) => store.save(record).is_ok(),
            _ => false,
        };
        app.history.save = if saved {
            SaveStatus::Saved
        } else {
            SaveStatus::Unsaved
        };
    }
    pub fn settle(&mut self, app: &mut App) -> HistoryResult<()> {
        if app.is_running() {
            return Ok(());
        }
        if let Some(completion) = app.take_completion() {
            self.pending = None;
            if self.enabled {
                self.pending = Record::from_report(
                    completion.id,
                    now_ms(),
                    &completion.report,
                    completion.exercise,
                )
                .ok();
                if self.pending.is_none() {
                    app.history.save = SaveStatus::Unsaved;
                }
                self.save(app);
            }
        }
        if app.pending_code && matches!(&app.screen,Screen::Practice(s) if s.phase()==Phase::Ready)
        {
            if let Some(store) = &mut self.store {
                match store.reserve_code(&mut app.selector.rng) {
                    Ok(prepared) => app.accept_reserved(prepared)?,
                    Err(_) => {
                        app.pending_code = false;
                        app.notice = Some("Code deck not saved; using in-memory selection.");
                    }
                }
            } else {
                app.pending_code = false;
            }
        }
        if app.history.retry {
            app.history.retry = false;
            self.save(app);
        }
        if app.history.clear_requested {
            app.history.clear_requested = false;
            if self.store.as_mut().is_some_and(|s| s.clear().is_ok()) {
                self.pending = None;
                app.history.save = SaveStatus::Cleared;
                app.notice = Some("History cleared; selection starts a new deck.");
                app.history.reset_page();
            } else {
                app.notice = Some("History could not be cleared; no success claimed.");
            }
        }
        if app.history.reload && matches!(app.screen, Screen::History { .. }) {
            app.history.reload = false;
            app.history.filter.now_ms = now_ms();
            let result = (|| -> HistoryResult<(Page, Stats)> {
                let store = self.store.as_ref().ok_or("history unavailable")?;
                let after = app.history.cursors.last().and_then(Option::as_ref);
                store.snapshot(&app.history.filter, after, app.history.filter.now_ms)
            })();
            match result {
                Ok((page, stats)) => {
                    app.history.page = page;
                    app.history.stats = stats;
                    app.history.unavailable = false;
                }
                Err(_) => {
                    app.history.page = Page::default();
                    app.history.stats = Stats::default();
                    app.history.unavailable = true;
                }
            }
        }
        Ok(())
    }
}
