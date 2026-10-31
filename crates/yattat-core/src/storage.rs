//! SQLite cache of issues and worklogs fetched from Jira, plus the coverage bookkeeping
//! (which days are already cached for which author).

use crate::error::{Error, Result};
use crate::models::{Issue, Project, Worklog};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS projects (
    key        TEXT PRIMARY KEY,
    id         TEXT NOT NULL,
    name       TEXT NOT NULL,
    avatar_url TEXT
);
CREATE TABLE IF NOT EXISTS issues (
    key                 TEXT PRIMARY KEY,
    id                  TEXT NOT NULL,
    project_key         TEXT NOT NULL,
    project_name        TEXT,
    summary             TEXT NOT NULL,
    issue_type          TEXT NOT NULL,
    issue_type_icon     TEXT,
    is_subtask          INTEGER NOT NULL DEFAULT 0,
    status              TEXT NOT NULL,
    status_category     TEXT NOT NULL,
    parent_key          TEXT,
    epic_key            TEXT,
    assignee            TEXT,
    assignee_id         TEXT,
    original_estimate_s INTEGER,
    time_spent_s        INTEGER,
    labels              TEXT NOT NULL DEFAULT '[]',
    components          TEXT NOT NULL DEFAULT '[]',
    updated             TEXT,
    fetched_at          TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS worklogs (
    id          TEXT PRIMARY KEY,
    issue_key   TEXT NOT NULL,
    author_id   TEXT NOT NULL,
    author_name TEXT,
    started     TEXT NOT NULL,
    date        TEXT NOT NULL,
    seconds     INTEGER NOT NULL,
    comment     TEXT NOT NULL DEFAULT '',
    updated     TEXT,
    fetched_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_worklogs_date ON worklogs(date);
CREATE INDEX IF NOT EXISTS idx_worklogs_author_date ON worklogs(author_id, date);
CREATE INDEX IF NOT EXISTS idx_worklogs_issue ON worklogs(issue_key);
CREATE TABLE IF NOT EXISTS coverage (
    author_id  TEXT NOT NULL,
    date       TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    PRIMARY KEY (author_id, date)
);
"#;

pub struct Store {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}

/// How completely the cache covers a range for a set of authors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CoverageInfo {
    pub total_days: usize,
    /// Days cached for every requested author.
    pub covered_days: usize,
    pub oldest_fetch: Option<DateTime<Utc>>,
    pub newest_fetch: Option<DateTime<Utc>>,
}

impl CoverageInfo {
    pub fn complete(&self) -> bool {
        self.covered_days >= self.total_days
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfo {
    pub worklogs: usize,
    pub issues: usize,
    pub coverage_days: usize,
    pub database_bytes: u64,
}

fn d(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| Error::InvalidDate(s.to_string()))
}

fn parse_utc(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn days(from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let mut out = Vec::new();
    let mut cur = from;
    while cur <= to {
        out.push(cur);
        cur += Duration::days(1);
    }
    out
}

fn issue_from_row(row: &Row<'_>) -> rusqlite::Result<Issue> {
    let labels: String = row.get("labels")?;
    let components: String = row.get("components")?;
    Ok(Issue {
        key: row.get("key")?,
        id: row.get("id")?,
        project_key: row.get("project_key")?,
        project_name: row.get("project_name")?,
        summary: row.get("summary")?,
        issue_type: row.get("issue_type")?,
        issue_type_icon: row.get("issue_type_icon")?,
        is_subtask: row.get::<_, i64>("is_subtask")? != 0,
        status: row.get("status")?,
        status_category: row.get("status_category")?,
        parent_key: row.get("parent_key")?,
        epic_key: row.get("epic_key")?,
        assignee: row.get("assignee")?,
        assignee_id: row.get("assignee_id")?,
        original_estimate_s: row.get("original_estimate_s")?,
        time_spent_s: row.get("time_spent_s")?,
        labels: serde_json::from_str(&labels).unwrap_or_default(),
        components: serde_json::from_str(&components).unwrap_or_default(),
        updated: row.get("updated")?,
    })
}

fn worklog_from_row(row: &Row<'_>) -> rusqlite::Result<Worklog> {
    let started: String = row.get("started")?;
    let date: String = row.get("date")?;
    let updated: Option<String> = row.get("updated")?;
    let parsed_started = DateTime::parse_from_rfc3339(&started).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let parsed_date = NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Worklog {
        id: row.get("id")?,
        issue_key: row.get("issue_key")?,
        author_id: row.get("author_id")?,
        author_name: row.get("author_name")?,
        started: parsed_started,
        date: parsed_date,
        seconds: row.get("seconds")?,
        comment: row.get("comment")?,
        updated: updated.as_deref().and_then(parse_utc),
    })
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let store = Self {
            conn: Mutex::new(conn),
            path: Some(path.to_path_buf()),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let store = Self {
            conn: Mutex::new(Connection::open_in_memory()?),
            path: None,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', '1') ON CONFLICT(key) DO NOTHING",
            [],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- meta

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        Ok(match rows.next()? {
            Some(row) => Some(row.get(0)?),
            None => None,
        })
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO meta(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------ projects

    pub fn upsert_projects(&self, projects: &[Project]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for p in projects {
            tx.execute(
                "INSERT INTO projects(key, id, name, avatar_url) VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(key) DO UPDATE SET id = excluded.id, name = excluded.name, avatar_url = excluded.avatar_url",
                params![p.key, p.id, p.name, p.avatar_url],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Remembers project names seen on issues. Existing rows only get their name refreshed.
    pub fn upsert_project_names(&self, names: &[(String, String)]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for (key, name) in names {
            tx.execute(
                "INSERT INTO projects(key, id, name, avatar_url) VALUES(?1, '', ?2, NULL)
                 ON CONFLICT(key) DO UPDATE SET name = excluded.name",
                params![key, name],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT key, id, name, avatar_url FROM projects ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            Ok(Project {
                key: row.get(0)?,
                id: row.get(1)?,
                name: row.get(2)?,
                avatar_url: row.get(3)?,
                color: None,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // -------------------------------------------------------------- issues

    pub fn upsert_issues(&self, issues: &[Issue], now: DateTime<Utc>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let fetched = now.to_rfc3339();
        for i in issues {
            tx.execute(
                "INSERT INTO issues(key, id, project_key, project_name, summary, issue_type, issue_type_icon, is_subtask, status,
                    status_category, parent_key, epic_key, assignee, assignee_id, original_estimate_s, time_spent_s,
                    labels, components, updated, fetched_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
                 ON CONFLICT(key) DO UPDATE SET id = excluded.id, project_key = excluded.project_key,
                    project_name = excluded.project_name, summary = excluded.summary, issue_type = excluded.issue_type, issue_type_icon = excluded.issue_type_icon,
                    is_subtask = excluded.is_subtask, status = excluded.status, status_category = excluded.status_category,
                    parent_key = excluded.parent_key, epic_key = excluded.epic_key, assignee = excluded.assignee,
                    assignee_id = excluded.assignee_id, original_estimate_s = excluded.original_estimate_s,
                    time_spent_s = excluded.time_spent_s, labels = excluded.labels, components = excluded.components,
                    updated = excluded.updated, fetched_at = excluded.fetched_at",
                params![
                    i.key,
                    i.id,
                    i.project_key,
                    i.project_name,
                    i.summary,
                    i.issue_type,
                    i.issue_type_icon,
                    i.is_subtask as i64,
                    i.status,
                    i.status_category,
                    i.parent_key,
                    i.epic_key,
                    i.assignee,
                    i.assignee_id,
                    i.original_estimate_s,
                    i.time_spent_s,
                    serde_json::to_string(&i.labels)?,
                    serde_json::to_string(&i.components)?,
                    i.updated,
                    fetched,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// All cached issues keyed by issue key.
    pub fn issue_map(&self) -> Result<HashMap<String, Issue>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM issues")?;
        let rows = stmt.query_map([], issue_from_row)?;
        let mut map = HashMap::new();
        for r in rows {
            let i = r?;
            map.insert(i.key.clone(), i);
        }
        Ok(map)
    }

    pub fn get_issue(&self, key: &str) -> Result<Option<Issue>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM issues WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], issue_from_row)?;
        Ok(match rows.next() {
            Some(r) => Some(r?),
            None => None,
        })
    }

    // ------------------------------------------------------------ worklogs

    pub fn upsert_worklogs(&self, worklogs: &[Worklog], now: DateTime<Utc>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let fetched = now.to_rfc3339();
        for w in worklogs {
            tx.execute(
                "INSERT INTO worklogs(id, issue_key, author_id, author_name, started, date, seconds, comment, updated, fetched_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(id) DO UPDATE SET issue_key = excluded.issue_key, author_id = excluded.author_id,
                    author_name = excluded.author_name, started = excluded.started, date = excluded.date,
                    seconds = excluded.seconds, comment = excluded.comment, updated = excluded.updated,
                    fetched_at = excluded.fetched_at",
                params![
                    w.id,
                    w.issue_key,
                    w.author_id,
                    w.author_name,
                    w.started.to_rfc3339(),
                    d(w.date),
                    w.seconds,
                    w.comment,
                    w.updated.map(|u| u.to_rfc3339()),
                    fetched,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Removes cached worklogs of `author_ids` within the range whose id is not in `keep`
    /// (they were deleted or moved in Jira). Returns the number of removed rows.
    pub fn prune_worklogs(
        &self,
        author_ids: &[String],
        from: NaiveDate,
        to: NaiveDate,
        keep: &HashSet<String>,
    ) -> Result<usize> {
        let authors: HashSet<&str> = author_ids.iter().map(String::as_str).collect();
        let mut conn = self.conn.lock().unwrap();
        let stale: Vec<String> = {
            let mut stmt =
                conn.prepare("SELECT id, author_id FROM worklogs WHERE date >= ?1 AND date <= ?2")?;
            let rows = stmt.query_map(params![d(from), d(to)], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut out = Vec::new();
            for r in rows {
                let (id, author) = r?;
                if authors.contains(author.as_str()) && !keep.contains(&id) {
                    out.push(id);
                }
            }
            out
        };
        let tx = conn.transaction()?;
        for id in &stale {
            tx.execute("DELETE FROM worklogs WHERE id = ?1", params![id])?;
        }
        tx.commit()?;
        Ok(stale.len())
    }

    /// Worklogs of the given authors between `from` and `to` (inclusive), oldest first.
    pub fn list_worklogs(
        &self,
        from: NaiveDate,
        to: NaiveDate,
        author_ids: &[String],
    ) -> Result<Vec<Worklog>> {
        if author_ids.is_empty() {
            return Ok(vec![]);
        }
        let authors: HashSet<&str> = author_ids.iter().map(String::as_str).collect();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT * FROM worklogs WHERE date >= ?1 AND date <= ?2 ORDER BY started, id",
        )?;
        let rows = stmt.query_map(params![d(from), d(to)], worklog_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            let w = r?;
            if authors.contains(w.author_id.as_str()) {
                out.push(w);
            }
        }
        Ok(out)
    }

    // ------------------------------------------------------------ coverage

    /// Days in the range that are not cached for `author_id`, ascending.
    pub fn missing_days(
        &self,
        author_id: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<NaiveDate>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT date FROM coverage WHERE author_id = ?1 AND date >= ?2 AND date <= ?3",
        )?;
        let rows = stmt.query_map(params![author_id, d(from), d(to)], |row| {
            row.get::<_, String>(0)
        })?;
        let mut covered: HashSet<NaiveDate> = HashSet::new();
        for r in rows {
            covered.insert(parse_date(&r?)?);
        }
        Ok(days(from, to)
            .into_iter()
            .filter(|day| !covered.contains(day))
            .collect())
    }

    pub fn mark_coverage(
        &self,
        author_ids: &[String],
        from: NaiveDate,
        to: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let fetched = now.to_rfc3339();
        for a in author_ids {
            for day in days(from, to) {
                tx.execute(
                    "INSERT INTO coverage(author_id, date, fetched_at) VALUES(?1, ?2, ?3)
                     ON CONFLICT(author_id, date) DO UPDATE SET fetched_at = excluded.fetched_at",
                    params![a, d(day), fetched],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn coverage(
        &self,
        author_ids: &[String],
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<CoverageInfo> {
        let all_days = days(from, to);
        let mut info = CoverageInfo {
            total_days: all_days.len(),
            ..Default::default()
        };
        if author_ids.is_empty() {
            info.covered_days = info.total_days;
            return Ok(info);
        }
        let authors: HashSet<&str> = author_ids.iter().map(String::as_str).collect();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT author_id, date, fetched_at FROM coverage WHERE date >= ?1 AND date <= ?2",
        )?;
        let rows = stmt.query_map(params![d(from), d(to)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut per_day: HashMap<String, usize> = HashMap::new();
        for r in rows {
            let (author, date, fetched) = r?;
            if !authors.contains(author.as_str()) {
                continue;
            }
            *per_day.entry(date).or_insert(0) += 1;
            if let Some(f) = parse_utc(&fetched) {
                info.oldest_fetch = Some(info.oldest_fetch.map_or(f, |o| o.min(f)));
                info.newest_fetch = Some(info.newest_fetch.map_or(f, |n| n.max(f)));
            }
        }
        info.covered_days = all_days
            .iter()
            .filter(|day| per_day.get(&d(**day)).copied().unwrap_or(0) >= authors.len())
            .count();
        Ok(info)
    }

    // ---------------------------------------------------------- maintenance

    pub fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "DELETE FROM worklogs; DELETE FROM issues; DELETE FROM coverage; DELETE FROM projects;",
        )?;
        let _ = conn.execute_batch("VACUUM;");
        Ok(())
    }

    pub fn cache_info(&self) -> Result<CacheInfo> {
        let conn = self.conn.lock().unwrap();
        let count = |sql: &str| -> rusqlite::Result<usize> {
            conn.query_row(sql, [], |r| r.get::<_, i64>(0))
                .map(|n| n as usize)
        };
        Ok(CacheInfo {
            worklogs: count("SELECT COUNT(*) FROM worklogs")?,
            issues: count("SELECT COUNT(*) FROM issues")?,
            coverage_days: count("SELECT COUNT(*) FROM coverage")?,
            database_bytes: self
                .path
                .as_ref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn issue(key: &str, project: &str) -> Issue {
        Issue {
            id: "1".into(),
            key: key.into(),
            project_key: project.into(),
            project_name: None,
            summary: format!("Summary {key}"),
            issue_type: "Task".into(),
            issue_type_icon: None,
            is_subtask: false,
            status: "Open".into(),
            status_category: "new".into(),
            parent_key: None,
            epic_key: None,
            assignee: None,
            assignee_id: None,
            original_estimate_s: Some(3600),
            time_spent_s: None,
            labels: vec!["x".into()],
            components: vec!["Auth".into()],
            updated: None,
        }
    }

    fn worklog(id: &str, key: &str, author: &str, date: &str, seconds: i64) -> Worklog {
        let started = DateTime::parse_from_rfc3339(&format!("{date}T09:00:00+01:00")).unwrap();
        Worklog {
            id: id.into(),
            issue_key: key.into(),
            author_id: author.into(),
            author_name: Some(author.to_uppercase()),
            started,
            date: started.date_naive(),
            seconds,
            comment: "c".into(),
            updated: Some(Utc::now()),
        }
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn issues_and_worklogs_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let now = Utc::now();
        store
            .upsert_issues(&[issue("A-1", "A"), issue("B-2", "B")], now)
            .unwrap();
        let map = store.issue_map().unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["A-1"].components, vec!["Auth".to_string()]);
        assert_eq!(store.get_issue("B-2").unwrap().unwrap().project_key, "B");
        assert!(store.get_issue("Z-9").unwrap().is_none());

        let logs = vec![
            worklog("1", "A-1", "alice", "2026-10-01", 3600),
            worklog("2", "A-1", "bob", "2026-10-02", 1800),
            worklog("3", "B-2", "alice", "2026-10-03", 600),
        ];
        store.upsert_worklogs(&logs, now).unwrap();
        // updating the same id replaces the row
        let mut changed = logs[0].clone();
        changed.seconds = 7200;
        store.upsert_worklogs(&[changed], now).unwrap();
        let alice = store
            .list_worklogs(day("2026-10-01"), day("2026-10-31"), &["alice".into()])
            .unwrap();
        assert_eq!(alice.len(), 2);
        assert_eq!(alice[0].seconds, 7200);
        assert_eq!(
            alice[0].started.offset(),
            &FixedOffset::east_opt(3600).unwrap()
        );
        let both = store
            .list_worklogs(
                day("2026-10-02"),
                day("2026-10-02"),
                &["alice".into(), "bob".into()],
            )
            .unwrap();
        assert_eq!(both.len(), 1);
        assert_eq!(both[0].author_id, "bob");
        assert!(store
            .list_worklogs(day("2026-10-01"), day("2026-10-31"), &[])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn prune_removes_only_unlisted_rows_of_given_authors() {
        let store = Store::open_in_memory().unwrap();
        let now = Utc::now();
        store
            .upsert_worklogs(
                &[
                    worklog("1", "A-1", "alice", "2026-10-01", 1),
                    worklog("2", "A-1", "alice", "2026-10-02", 1),
                    worklog("3", "A-1", "bob", "2026-10-02", 1),
                    worklog("4", "A-1", "alice", "2026-11-02", 1),
                ],
                now,
            )
            .unwrap();
        let keep: HashSet<String> = ["1".to_string()].into_iter().collect();
        let removed = store
            .prune_worklogs(
                &["alice".into()],
                day("2026-10-01"),
                day("2026-10-31"),
                &keep,
            )
            .unwrap();
        assert_eq!(removed, 1);
        let ids: Vec<String> = store
            .list_worklogs(
                day("2026-01-01"),
                day("2026-12-31"),
                &["alice".into(), "bob".into()],
            )
            .unwrap()
            .into_iter()
            .map(|w| w.id)
            .collect();
        assert_eq!(ids, vec!["1", "3", "4"]);
    }

    #[test]
    fn coverage_bookkeeping() {
        let store = Store::open_in_memory().unwrap();
        let now = Utc::now();
        let from = day("2026-10-01");
        let to = day("2026-10-05");
        assert_eq!(store.missing_days("alice", from, to).unwrap().len(), 5);
        store
            .mark_coverage(&["alice".into()], from, day("2026-10-03"), now)
            .unwrap();
        assert_eq!(
            store.missing_days("alice", from, to).unwrap(),
            vec![day("2026-10-04"), day("2026-10-05")]
        );
        let info = store.coverage(&["alice".into()], from, to).unwrap();
        assert_eq!(info.total_days, 5);
        assert_eq!(info.covered_days, 3);
        assert!(!info.complete());
        assert!(info.newest_fetch.is_some());
        // a second author without coverage means no day is fully covered
        let info = store
            .coverage(&["alice".into(), "bob".into()], from, to)
            .unwrap();
        assert_eq!(info.covered_days, 0);
        store
            .mark_coverage(&["alice".into(), "bob".into()], from, to, now)
            .unwrap();
        assert!(store
            .coverage(&["alice".into(), "bob".into()], from, to)
            .unwrap()
            .complete());
        // without authors there is nothing to fetch
        assert!(store.coverage(&[], from, to).unwrap().complete());
        let info = store.cache_info().unwrap();
        assert_eq!(info.coverage_days, 10);
        store.clear().unwrap();
        assert_eq!(store.cache_info().unwrap().coverage_days, 0);
    }

    #[test]
    fn opens_file_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("cache.sqlite");
        let store = Store::open(&path).unwrap();
        store.set_meta("a", "b").unwrap();
        assert_eq!(store.get_meta("a").unwrap().as_deref(), Some("b"));
        assert_eq!(store.get_meta("x").unwrap(), None);
        assert!(store.cache_info().unwrap().database_bytes > 0);
        store
            .upsert_projects(&[Project {
                id: "1".into(),
                key: "A".into(),
                name: "Alpha".into(),
                avatar_url: None,
                color: None,
            }])
            .unwrap();
        assert_eq!(store.list_projects().unwrap()[0].name, "Alpha");
        store
            .upsert_project_names(&[("A".into(), "Alpha 2".into()), ("B".into(), "Beta".into())])
            .unwrap();
        let projects = store.list_projects().unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "Alpha 2");
        assert_eq!(projects[0].id, "1");
        assert_eq!(projects[1].id, "");
    }
}
