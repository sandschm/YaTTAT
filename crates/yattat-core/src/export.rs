//! CSV/JSON export of worklogs, optionally summarized per day, member and ticket.

use crate::error::Result;
use crate::i18n::Lang;
use crate::models::{Issue, Member, Worklog};
use crate::settings::TimeFormat;
use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportRow {
    pub date: NaiveDate,
    pub started: DateTime<FixedOffset>,
    pub member_id: String,
    pub member: String,
    pub project_key: String,
    pub issue_key: String,
    pub summary: String,
    pub issue_type: String,
    pub seconds: i64,
    pub comment: String,
    pub worklog_ids: Vec<String>,
}

pub fn build_rows(
    worklogs: &[Worklog],
    issues: &HashMap<String, Issue>,
    members: &[Member],
) -> Vec<ExportRow> {
    let names: HashMap<&str, &str> = members
        .iter()
        .map(|m| (m.id.as_str(), m.display_name.as_str()))
        .collect();
    let mut rows: Vec<ExportRow> = worklogs
        .iter()
        .map(|w| {
            let issue = issues.get(&w.issue_key);
            ExportRow {
                date: w.date,
                started: w.started,
                member_id: w.author_id.clone(),
                member: names
                    .get(w.author_id.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| w.author_name.clone())
                    .unwrap_or_else(|| w.author_id.clone()),
                project_key: issue.map(|i| i.project_key.clone()).unwrap_or_else(|| {
                    w.issue_key
                        .rsplit_once('-')
                        .map(|(p, _)| p.to_string())
                        .unwrap_or_default()
                }),
                issue_key: w.issue_key.clone(),
                summary: issue.map(|i| i.summary.clone()).unwrap_or_default(),
                issue_type: issue.map(|i| i.issue_type.clone()).unwrap_or_default(),
                seconds: w.seconds,
                comment: w.comment.clone(),
                worklog_ids: vec![w.id.clone()],
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        a.started
            .cmp(&b.started)
            .then_with(|| a.issue_key.cmp(&b.issue_key))
    });
    rows
}

/// One row per day, member and ticket. Comments are joined.
pub fn summarize_rows(rows: &[ExportRow]) -> Vec<ExportRow> {
    let mut map: BTreeMap<(NaiveDate, String, String), ExportRow> = BTreeMap::new();
    for r in rows {
        let key = (r.date, r.member_id.clone(), r.issue_key.clone());
        match map.get_mut(&key) {
            Some(g) => {
                g.seconds += r.seconds;
                g.worklog_ids.extend(r.worklog_ids.iter().cloned());
                if !r.comment.is_empty() && !g.comment.contains(&r.comment) {
                    if g.comment.is_empty() {
                        g.comment = r.comment.clone();
                    } else {
                        g.comment = format!("{} | {}", g.comment, r.comment);
                    }
                }
                if r.started < g.started {
                    g.started = r.started;
                }
            }
            None => {
                map.insert(key, r.clone());
            }
        }
    }
    let mut out: Vec<ExportRow> = map.into_values().collect();
    out.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then_with(|| a.member.cmp(&b.member))
            .then_with(|| a.issue_key.cmp(&b.issue_key))
    });
    out
}

/// CSV with a semicolon delimiter and a BOM (Excel), headers and date format by language.
pub fn to_csv(rows: &[ExportRow], format: TimeFormat, lang: Lang) -> Result<String> {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b';')
        .from_writer(vec![]);
    let (headers, date_format): ([&str; 11], &str) = match lang {
        Lang::De => (
            [
                "Datum",
                "Beginn",
                "Mitglied",
                "Projekt",
                "Ticket",
                "Titel",
                "Typ",
                "Dauer",
                "Dauer (Sekunden)",
                "Kommentar",
                "Worklog-IDs",
            ],
            "%d.%m.%Y",
        ),
        Lang::En => (
            [
                "Date",
                "Start",
                "Member",
                "Project",
                "Issue",
                "Summary",
                "Type",
                "Duration",
                "Duration (seconds)",
                "Comment",
                "Worklog IDs",
            ],
            "%Y-%m-%d",
        ),
    };
    wtr.write_record(headers)?;
    for r in rows {
        wtr.write_record([
            r.date.format(date_format).to_string(),
            r.started.format("%H:%M").to_string(),
            r.member.clone(),
            r.project_key.clone(),
            r.issue_key.clone(),
            r.summary.clone(),
            r.issue_type.clone(),
            crate::format_duration(r.seconds, format, lang),
            r.seconds.to_string(),
            r.comment.replace(['\n', '\r'], " "),
            r.worklog_ids.join(","),
        ])?;
    }
    let bytes = wtr
        .into_inner()
        .map_err(|e| crate::Error::Io(e.to_string()))?;
    // BOM so that Excel opens the UTF-8 file correctly.
    Ok(format!("\u{feff}{}", String::from_utf8_lossy(&bytes)))
}

pub fn to_json(rows: &[ExportRow]) -> Result<String> {
    Ok(serde_json::to_string_pretty(rows)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worklog(
        id: &str,
        key: &str,
        author: &str,
        ts: &str,
        seconds: i64,
        comment: &str,
    ) -> Worklog {
        let started = DateTime::parse_from_rfc3339(ts).unwrap();
        Worklog {
            id: id.into(),
            issue_key: key.into(),
            author_id: author.into(),
            author_name: Some("From Jira".into()),
            started,
            date: started.date_naive(),
            seconds,
            comment: comment.into(),
            updated: None,
        }
    }

    #[test]
    fn rows_summary_and_csv() {
        let members = vec![Member {
            id: "alice".into(),
            display_name: "Alice".into(),
            ..Default::default()
        }];
        let mut issues = HashMap::new();
        issues.insert(
            "A-1".to_string(),
            Issue {
                id: "1".into(),
                key: "A-1".into(),
                project_key: "A".into(),
                project_name: None,
                summary: "Semi;colon".into(),
                issue_type: "Bug".into(),
                issue_type_icon: None,
                is_subtask: false,
                status: "Open".into(),
                status_category: "new".into(),
                parent_key: None,
                epic_key: None,
                assignee: None,
                assignee_id: None,
                original_estimate_s: None,
                time_spent_s: None,
                labels: vec![],
                components: vec![],
                updated: None,
            },
        );
        let logs = vec![
            worklog(
                "2",
                "A-1",
                "alice",
                "2026-10-05T14:00:00+02:00",
                1800,
                "later",
            ),
            worklog(
                "1",
                "A-1",
                "alice",
                "2026-10-05T09:00:00+02:00",
                3600,
                "first\nline",
            ),
            worklog("3", "B-7", "bob", "2026-10-05T09:30:00+02:00", 600, ""),
        ];
        let rows = build_rows(&logs, &issues, &members);
        assert_eq!(rows[0].worklog_ids, vec!["1"]);
        assert_eq!(rows[1].member, "From Jira");
        assert_eq!(rows[1].project_key, "B");
        let summary = summarize_rows(&rows);
        assert_eq!(summary.len(), 2);
        let alice = summary.iter().find(|r| r.member_id == "alice").unwrap();
        assert_eq!(alice.seconds, 5400);
        assert_eq!(alice.comment, "first\nline | later");
        assert_eq!(alice.worklog_ids, vec!["1", "2"]);
        assert_eq!(alice.started.format("%H:%M").to_string(), "09:00");
        let csv = to_csv(&summary, TimeFormat::HoursMinutes, Lang::De).unwrap();
        assert!(csv.starts_with("\u{feff}Datum;"));
        assert!(csv.contains("\"Semi;colon\""));
        assert!(csv.contains(";1:30;5400;first line | later;"));
        assert!(csv.contains("05.10.2026;"));
        let csv_en = to_csv(&summary, TimeFormat::Decimal, Lang::En).unwrap();
        assert!(csv_en.starts_with("\u{feff}Date;"));
        assert!(csv_en.contains("2026-10-05;"));
        assert!(csv_en.contains(";1.50;5400;"));
        let json = to_json(&rows).unwrap();
        assert!(json.contains("\"issueKey\": \"A-1\""));
    }
}
