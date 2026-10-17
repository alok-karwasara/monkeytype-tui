use super::{
    model::*,
    path::{private_directory, private_file, regular_or_missing},
};
use crate::{
    content::{self, Exercise, Prepared},
    engine::{FinishReason, Mode},
};
use rand::{rngs::SmallRng, seq::SliceRandom};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, TransactionBehavior, named_params, params,
};
use std::{
    collections::HashSet,
    fs::{self, File},
    path::Path,
    time::Duration,
};

const SCHEMA: &str = include_str!("schema.sql");
const COLUMNS: &str = "id,completed_ms,mode,seconds,elapsed_ns,reason,exercise_id,exercise_version,pool,pool_version,language,attempts,correct_attempts,current_correct,backspaces,uncorrected,wpm,raw_wpm,accuracy,metric_version,app_version";
const WHERE_FILTER: &str = "metric_version=:metric AND pool_version=:pool_version
 AND (:mode IS NULL OR mode=:mode) AND (:seconds IS NULL OR seconds=:seconds)
 AND (:exercise IS NULL OR exercise_id=:exercise) AND (:language IS NULL OR language=:language)
 AND (:exercise_version IS NULL OR exercise_version=:exercise_version)
 AND (:reason IS NULL OR reason=:reason) AND completed_ms>=:since";

fn reason_name(reason: FinishReason) -> &'static str {
    match reason {
        FinishReason::Timeout => "timeout",
        FinishReason::SnippetExhausted => "snippet",
    }
}
fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<Record> {
    let bad = || rusqlite::Error::InvalidQuery;
    let mode = match row.get::<_, String>(2)?.as_str() {
        "words" => Mode::Words,
        "code" => Mode::Code,
        _ => return Err(bad()),
    };
    let reason = match row.get::<_, String>(5)?.as_str() {
        "timeout" => FinishReason::Timeout,
        "snippet" => FinishReason::SnippetExhausted,
        _ => return Err(bad()),
    };
    Ok(Record {
        id: row.get(0)?,
        completed_ms: row.get(1)?,
        mode,
        seconds: row.get(3)?,
        elapsed_ns: row.get(4)?,
        reason,
        exercise: Exercise {
            id: row.get(6)?,
            version: row.get(7)?,
            pool: row.get(8)?,
            pool_version: row.get(9)?,
            language: row.get(10)?,
        },
        attempts: row.get(11)?,
        correct_attempts: row.get(12)?,
        current_correct: row.get(13)?,
        backspaces: row.get(14)?,
        uncorrected: row.get(15)?,
        wpm: row.get(16)?,
        raw_wpm: row.get(17)?,
        accuracy: row.get(18)?,
        metric_version: row.get(19)?,
        app_version: row.get(20)?,
    })
}
macro_rules! filters {($f:expr)=>{named_params!{
    ":metric":$f.metric_version,":pool_version":$f.pool_version,":mode":$f.mode.map(|m|m.to_string()),
    ":seconds":$f.seconds,":exercise":$f.exercise,":language":$f.language,":exercise_version":$f.exercise_version,
    ":reason":$f.reason.map(reason_name),":since":$f.since(),
}};}

pub struct Store {
    connection: Connection,
    _lock: File,
}
impl Store {
    pub fn snapshot(
        &self,
        f: &Filter,
        after: Option<&Cursor>,
        now: i64,
    ) -> HistoryResult<(Page, Stats)> {
        let tx = self.connection.unchecked_transaction()?;
        let page = self.page(f, after)?;
        let stats = self.stats(f, now)?;
        tx.commit()?;
        Ok((page, stats))
    }
    pub fn open(directory: &Path) -> HistoryResult<Self> {
        private_directory(directory, true)?;
        let lock = private_file(&directory.join("history.lock"))?;
        lock.try_lock_shared()
            .map_err(|_| "history is being purged")?;
        for name in [
            "history.sqlite3",
            "history.sqlite3-journal",
            "history.sqlite3-wal",
            "history.sqlite3-shm",
        ] {
            regular_or_missing(&directory.join(name))?;
        }
        let db = directory.join("history.sqlite3");
        drop(private_file(&db)?);
        let mut connection = Connection::open_with_flags(
            db,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NOFOLLOW
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::from_millis(100))?;
        migrate(&mut connection, SCHEMA)?;
        connection.prepare(&format!("SELECT {COLUMNS} FROM sessions LIMIT 0"))?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(Self {
            connection,
            _lock: lock,
        })
    }

    pub fn save(&mut self, record: &Record) -> HistoryResult<bool> {
        record.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = tx
            .query_row(
                &format!("SELECT {COLUMNS} FROM sessions WHERE id=?1"),
                [&record.id],
                decode,
            )
            .optional()?;
        if let Some(existing) = existing {
            existing.validate()?;
            if existing != *record {
                return Err("session ID already belongs to different results".into());
            }
            tx.commit()?;
            return Ok(false);
        }
        tx.execute("INSERT INTO sessions VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
            params![record.id,record.completed_ms,record.mode.to_string(),record.seconds,record.elapsed_ns,reason_name(record.reason),
                record.exercise.id,record.exercise.version,record.exercise.pool,record.exercise.pool_version,record.exercise.language,
                record.attempts,record.correct_attempts,record.current_correct,record.backspaces,record.uncorrected,
                record.wpm,record.raw_wpm,record.accuracy,record.metric_version,record.app_version])?;
        tx.commit()?;
        Ok(true)
    }

    pub fn page(&self, f: &Filter, after: Option<&Cursor>) -> HistoryResult<Page> {
        f.validate()?;
        let sql = format!(
            "SELECT {COLUMNS} FROM sessions WHERE {WHERE_FILTER}
            AND (:stamp IS NULL OR completed_ms<:stamp OR (completed_ms=:stamp AND id<:id))
            ORDER BY completed_ms DESC,id DESC LIMIT 51"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows=statement.query_map(named_params!{
            ":metric":f.metric_version,":pool_version":f.pool_version,":mode":f.mode.map(|m|m.to_string()),":seconds":f.seconds,
            ":exercise":f.exercise,":exercise_version":f.exercise_version,":language":f.language,":reason":f.reason.map(reason_name),":since":f.since(),
            ":stamp":after.map(|v|v.0),":id":after.map(|v|v.1.as_str()),
        },decode)?;
        let mut records = Vec::new();
        for row in rows {
            let r = row?;
            r.validate()?;
            records.push(r);
        }
        let more = records.len() > 50;
        if more {
            records.pop();
        }
        let next = if more {
            records.last().map(|r| (r.completed_ms, r.id.clone()))
        } else {
            None
        };
        Ok(Page {
            rows: records,
            next,
        })
    }

    pub fn stats(&self, f: &Filter, now: i64) -> HistoryResult<Stats> {
        f.validate()?;
        let mut stats = self.connection.query_row(
            &format!(
                "SELECT count(*),total(elapsed_ns),max(wpm),
            CASE WHEN total(attempts)>0 THEN 100.0*total(correct_attempts)/total(attempts) END
            FROM sessions WHERE {WHERE_FILTER}"
            ),
            filters!(f),
            |r| {
                Ok(Stats {
                    count: r.get(0)?,
                    practice_ns: r.get(1)?,
                    best: r.get(2)?,
                    accuracy: r.get(3)?,
                    ..Stats::default()
                })
            },
        )?;
        stats.median = self.connection.query_row(
            &format!(
                "SELECT avg(wpm) FROM (
            SELECT wpm,row_number() OVER (ORDER BY wpm) AS n,count(*) OVER () AS total
            FROM sessions WHERE {WHERE_FILTER} AND wpm IS NOT NULL)
            WHERE n IN ((total+1)/2,(total+2)/2)"
            ),
            filters!(f),
            |r| r.get(0),
        )?;
        let mut q=self.connection.prepare(&format!("SELECT completed_ms,wpm,accuracy FROM sessions WHERE {WHERE_FILTER} AND wpm IS NOT NULL ORDER BY completed_ms DESC,id DESC LIMIT 50"))?;
        stats.trend = q
            .query_map(filters!(f), |r| {
                Ok((
                    r.get::<_, i64>(0)? as f64,
                    r.get::<_, f64>(1)?,
                    r.get::<_, Option<f64>>(2)?.unwrap_or(f64::NAN),
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        stats.trend.reverse();
        if !f.comparable() {
            stats.best = None;
        } else if stats.trend.len() >= 20 {
            let tail = &stats.trend[stats.trend.len() - 20..];
            stats.change = Some(
                tail[10..].iter().map(|p| p.1).sum::<f64>() / 10.0
                    - tail[..10].iter().map(|p| p.1).sum::<f64>() / 10.0,
            );
        }
        stats.daily = vec![0.0; 30];
        let first = now.div_euclid(DAY_MS) - 29;
        let mut q=self.connection.prepare(&format!("SELECT completed_ms/{DAY_MS},total(elapsed_ns)/1000000000.0 FROM sessions WHERE {WHERE_FILTER} AND completed_ms/:day BETWEEN :first AND :last GROUP BY completed_ms/:day"))?;
        let rows=q.query_map(named_params!{
            ":metric":f.metric_version,":pool_version":f.pool_version,":mode":f.mode.map(|m|m.to_string()),":seconds":f.seconds,":exercise":f.exercise,":exercise_version":f.exercise_version,
            ":language":f.language,":reason":f.reason.map(reason_name),":since":f.since(),":day":DAY_MS,":first":first,":last":first+29,
        },|r|Ok((r.get::<_,i64>(0)?,r.get::<_,f64>(1)?)))?;
        for row in rows {
            let (day, value) = row?;
            if let Some(slot) = stats.daily.get_mut((day - first) as usize) {
                *slot = value;
            }
        }
        Ok(stats)
    }

    pub fn clear(&mut self) -> HistoryResult<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(
            "DELETE FROM sessions; DELETE FROM content_selection; DELETE FROM content_meta;",
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn reserve_code(&mut self, rng: &mut SmallRng) -> HistoryResult<Prepared> {
        let catalog = content::catalog();
        let eligible: HashSet<&str> = catalog.iter().map(|e| e.meta.id.as_str()).collect();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let meta = tx
            .query_row(
                "SELECT version,last_id FROM content_meta WHERE singleton=1",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        let mut deck = tx
            .prepare("SELECT exercise_id FROM content_selection ORDER BY position")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if meta.as_ref().is_none_or(|m| m.0 != 1)
            || deck.len() > catalog.len()
            || deck.iter().any(|id| !eligible.contains(id.as_str()))
            || deck.iter().collect::<HashSet<_>>().len() != deck.len()
        {
            deck.clear();
        }
        if deck.is_empty() {
            deck = catalog.iter().map(|e| e.meta.id.clone()).collect();
            deck.shuffle(rng);
            if deck.len() > 1 && meta.as_ref().is_some_and(|m| deck[0] == m.1) {
                deck.swap(0, 1);
            }
        }
        let chosen = deck.first().cloned().ok_or("empty code catalog")?;
        let prepared = catalog
            .iter()
            .find(|e| e.meta.id == chosen)
            .ok_or("invalid selected exercise")?
            .clone();
        tx.execute("DELETE FROM content_selection", [])?;
        for (position, id) in deck.iter().skip(1).enumerate() {
            tx.execute(
                "INSERT INTO content_selection VALUES(?1,?2)",
                params![position as i64, id],
            )?;
        }
        tx.execute("INSERT INTO content_meta VALUES(1,1,?1) ON CONFLICT(singleton) DO UPDATE SET version=1,last_id=excluded.last_id",[&chosen])?;
        tx.commit()?;
        Ok(prepared)
    }
}

fn migrate(connection: &mut Connection, sql: &str) -> HistoryResult<()> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
    match version {
        0 => {
            let count: i64 = tx.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )?;
            if count != 0 {
                return Err("unrecognized history database; preserved without migration".into());
            }
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", 1)?;
        }
        1 => {}
        _ => return Err("unsupported history schema; database preserved".into()),
    }
    tx.commit()?;
    Ok(())
}

pub fn purge(directory: &Path) -> HistoryResult<()> {
    if !directory.try_exists()? {
        return Ok(());
    }
    private_directory(directory, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(directory)?.permissions().mode() & 0o200 == 0 {
            return Err("history directory is read-only; data preserved".into());
        }
    }
    let lock = private_file(&directory.join("history.lock"))?;
    lock.try_lock()
        .map_err(|_| "history is in use; close other app instances before purge")?;
    let files = [
        "history.sqlite3",
        "history.sqlite3-journal",
        "history.sqlite3-wal",
        "history.sqlite3-shm",
    ];
    for name in files {
        regular_or_missing(&directory.join(name))?;
    }
    for name in files {
        let path = directory.join(name);
        if regular_or_missing(&path)? {
            fs::remove_file(path)?;
        }
    }
    // Keep the empty lock file: unlinking it would permit independent lock inodes.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn his17_failed_migration_rolls_back_schema_and_version() -> HistoryResult<()> {
        let mut c = Connection::open_in_memory()?;
        assert!(migrate(&mut c, "CREATE TABLE partial(value INTEGER); INVALID SQL;").is_err());
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='partial'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        assert_eq!(
            c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
    #[test]
    fn his17_schema_initialization_rechecks_version_under_transaction() -> HistoryResult<()> {
        let mut c = Connection::open_in_memory()?;
        migrate(&mut c, SCHEMA)?;
        migrate(&mut c, SCHEMA)?;
        assert_eq!(
            c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
            1
        );
        Ok(())
    }
}
