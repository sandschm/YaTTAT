//! Aggregations for the team, member and ticket views. Day attribution follows the worklog's own
//! date (the author's time zone), targets come from the work settings and the member's weekly hours.

use crate::models::{Issue, Member, Project, Worklog};
use crate::settings::WorkSettings;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroupStat {
    pub key: String,
    pub label: String,
    pub seconds: i64,
    pub project_key: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    pub date: NaiveDate,
    pub seconds: i64,
    pub target_seconds: i64,
    /// Member id -> seconds on that day.
    pub by_member: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemberStat {
    pub id: String,
    pub display_name: String,
    pub color: Option<String>,
    pub seconds: i64,
    pub target_seconds: i64,
    pub worklog_count: usize,
    pub issue_count: usize,
    pub days_logged: usize,
    pub avg_per_logged_day_seconds: i64,
    pub last_worklog: Option<DateTime<Utc>>,
    pub by_project: Vec<GroupStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IssueStat {
    pub key: String,
    pub summary: String,
    pub project_key: String,
    pub issue_type: String,
    pub status: String,
    pub status_category: String,
    pub parent_key: Option<String>,
    pub epic_key: Option<String>,
    pub assignee: Option<String>,
    pub seconds: i64,
    pub worklog_count: usize,
    /// Member id -> seconds.
    pub by_member: BTreeMap<String, i64>,
    pub original_estimate_s: Option<i64>,
    /// Jira's total for the issue (all authors, all time) at fetch time.
    pub time_spent_s: Option<i64>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub total_seconds: i64,
    pub target_seconds: i64,
    pub worklog_count: usize,
    pub issue_count: usize,
    /// Distinct days with at least one worklog.
    pub days_logged: usize,
    /// Days up to today with a target > 0.
    pub work_days: usize,
    pub members: Vec<MemberStat>,
    pub days: Vec<DayStat>,
    pub by_project: Vec<GroupStat>,
    pub by_issue: Vec<IssueStat>,
    pub by_issue_type: Vec<GroupStat>,
    /// Monday..Sunday
    pub by_weekday: [i64; 7],
}

/// Target seconds of a member on a date: the weekday pattern, scaled by the member's weekly hours.
pub fn member_target_seconds(member: &Member, date: NaiveDate, work: &WorkSettings) -> i64 {
    let idx = date.weekday().num_days_from_monday() as usize;
    let base = work.hours_per_weekday.get(idx).copied().unwrap_or(0.0);
    let pattern = work.pattern_hours_per_week();
    let factor = match member.weekly_hours {
        Some(weekly) if pattern > 0.0 => weekly / pattern,
        _ => 1.0,
    };
    (base * factor * 3600.0).round() as i64
}

fn project_of(issue: Option<&Issue>, key: &str) -> String {
    issue.map(|i| i.project_key.clone()).unwrap_or_else(|| {
        key.rsplit_once('-')
            .map(|(p, _)| p.to_string())
            .unwrap_or_default()
    })
}

fn group_stats(map: HashMap<String, i64>, projects: &HashMap<String, Project>) -> Vec<GroupStat> {
    let mut out: Vec<GroupStat> = map
        .into_iter()
        .map(|(key, seconds)| {
            let p = projects.get(&key);
            GroupStat {
                label: p.map(|p| p.name.clone()).unwrap_or_else(|| key.clone()),
                color: p.and_then(|p| p.color.clone()),
                project_key: Some(key.clone()),
                key,
                seconds,
            }
        })
        .collect();
    out.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.key.cmp(&b.key)));
    out
}

#[allow(clippy::too_many_arguments)]
pub fn compute(
    worklogs: &[Worklog],
    issues: &HashMap<String, Issue>,
    projects: &HashMap<String, Project>,
    members: &[Member],
    from: NaiveDate,
    to: NaiveDate,
    work: &WorkSettings,
    today: NaiveDate,
) -> Stats {
    let member_ids: HashSet<&str> = members.iter().map(|m| m.id.as_str()).collect();
    let mut days: BTreeMap<NaiveDate, DayStat> = BTreeMap::new();
    let mut member_targets: HashMap<String, i64> = HashMap::new();
    let mut d = from;
    while d <= to {
        let mut target = 0;
        if d <= today {
            for m in members {
                let t = member_target_seconds(m, d, work);
                *member_targets.entry(m.id.clone()).or_insert(0) += t;
                target += t;
            }
        }
        days.insert(
            d,
            DayStat {
                date: d,
                seconds: 0,
                target_seconds: target,
                by_member: BTreeMap::new(),
            },
        );
        d += Duration::days(1);
    }

    struct MemberAcc {
        seconds: i64,
        worklogs: usize,
        issues: HashSet<String>,
        days: HashSet<NaiveDate>,
        last: Option<DateTime<Utc>>,
        by_project: HashMap<String, i64>,
    }
    let mut acc: HashMap<String, MemberAcc> = HashMap::new();
    let mut by_project: HashMap<String, i64> = HashMap::new();
    let mut by_type: HashMap<String, i64> = HashMap::new();
    let mut by_weekday = [0i64; 7];
    struct IssueAcc {
        seconds: i64,
        count: usize,
        by_member: BTreeMap<String, i64>,
    }
    let mut issue_acc: HashMap<String, IssueAcc> = HashMap::new();
    let mut total = 0i64;
    let mut count = 0usize;
    let mut logged_days: HashSet<NaiveDate> = HashSet::new();

    for w in worklogs {
        if w.date < from || w.date > to || !member_ids.contains(w.author_id.as_str()) {
            continue;
        }
        let issue = issues.get(&w.issue_key);
        let project = project_of(issue, &w.issue_key);
        total += w.seconds;
        count += 1;
        logged_days.insert(w.date);
        if let Some(day) = days.get_mut(&w.date) {
            day.seconds += w.seconds;
            *day.by_member.entry(w.author_id.clone()).or_insert(0) += w.seconds;
        }
        by_weekday[w.date.weekday().num_days_from_monday() as usize] += w.seconds;
        *by_project.entry(project.clone()).or_insert(0) += w.seconds;
        *by_type
            .entry(
                issue
                    .map(|i| i.issue_type.clone())
                    .unwrap_or_else(|| "Unbekannt".into()),
            )
            .or_insert(0) += w.seconds;
        let m = acc.entry(w.author_id.clone()).or_insert_with(|| MemberAcc {
            seconds: 0,
            worklogs: 0,
            issues: HashSet::new(),
            days: HashSet::new(),
            last: None,
            by_project: HashMap::new(),
        });
        m.seconds += w.seconds;
        m.worklogs += 1;
        m.issues.insert(w.issue_key.clone());
        m.days.insert(w.date);
        let started_utc = w.started.with_timezone(&Utc);
        m.last = Some(m.last.map_or(started_utc, |l| l.max(started_utc)));
        *m.by_project.entry(project).or_insert(0) += w.seconds;
        let ia = issue_acc
            .entry(w.issue_key.clone())
            .or_insert_with(|| IssueAcc {
                seconds: 0,
                count: 0,
                by_member: BTreeMap::new(),
            });
        ia.seconds += w.seconds;
        ia.count += 1;
        *ia.by_member.entry(w.author_id.clone()).or_insert(0) += w.seconds;
    }

    let member_stats: Vec<MemberStat> = members
        .iter()
        .map(|m| {
            let a = acc.remove(&m.id);
            let seconds = a.as_ref().map(|a| a.seconds).unwrap_or(0);
            let days_logged = a.as_ref().map(|a| a.days.len()).unwrap_or(0);
            MemberStat {
                id: m.id.clone(),
                display_name: m.display_name.clone(),
                color: m.color.clone(),
                seconds,
                target_seconds: member_targets.get(&m.id).copied().unwrap_or(0),
                worklog_count: a.as_ref().map(|a| a.worklogs).unwrap_or(0),
                issue_count: a.as_ref().map(|a| a.issues.len()).unwrap_or(0),
                days_logged,
                avg_per_logged_day_seconds: if days_logged > 0 {
                    seconds / days_logged as i64
                } else {
                    0
                },
                last_worklog: a.as_ref().and_then(|a| a.last),
                by_project: group_stats(a.map(|a| a.by_project).unwrap_or_default(), projects),
            }
        })
        .collect();

    let mut by_issue: Vec<IssueStat> = issue_acc
        .into_iter()
        .map(|(key, a)| {
            let issue = issues.get(&key);
            let project_key = project_of(issue, &key);
            IssueStat {
                summary: issue.map(|i| i.summary.clone()).unwrap_or_default(),
                issue_type: issue.map(|i| i.issue_type.clone()).unwrap_or_default(),
                status: issue.map(|i| i.status.clone()).unwrap_or_default(),
                status_category: issue
                    .map(|i| i.status_category.clone())
                    .unwrap_or_else(|| "undefined".into()),
                parent_key: issue.and_then(|i| i.parent_key.clone()),
                epic_key: issue.and_then(|i| i.epic_key.clone()),
                assignee: issue.and_then(|i| i.assignee.clone()),
                seconds: a.seconds,
                worklog_count: a.count,
                by_member: a.by_member,
                original_estimate_s: issue.and_then(|i| i.original_estimate_s),
                time_spent_s: issue.and_then(|i| i.time_spent_s),
                color: projects.get(&project_key).and_then(|p| p.color.clone()),
                project_key,
                key,
            }
        })
        .collect();
    by_issue.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.key.cmp(&b.key)));

    let mut by_issue_type: Vec<GroupStat> = by_type
        .into_iter()
        .map(|(key, seconds)| GroupStat {
            label: key.clone(),
            key,
            seconds,
            project_key: None,
            color: None,
        })
        .collect();
    by_issue_type.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.key.cmp(&b.key)));

    let days: Vec<DayStat> = days.into_values().collect();
    Stats {
        from,
        to,
        total_seconds: total,
        target_seconds: days.iter().map(|d| d.target_seconds).sum(),
        worklog_count: count,
        issue_count: by_issue.len(),
        days_logged: logged_days.len(),
        work_days: days.iter().filter(|d| d.target_seconds > 0).count(),
        members: member_stats,
        days,
        by_project: group_stats(by_project, projects),
        by_issue,
        by_issue_type,
        by_weekday,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, weekly: Option<f32>) -> Member {
        Member {
            id: id.into(),
            display_name: id.to_uppercase(),
            weekly_hours: weekly,
            color: Some("#000".into()),
            ..Default::default()
        }
    }

    fn issue(key: &str, project: &str, kind: &str) -> Issue {
        Issue {
            id: "1".into(),
            key: key.into(),
            project_key: project.into(),
            project_name: None,
            summary: format!("S {key}"),
            issue_type: kind.into(),
            issue_type_icon: None,
            is_subtask: false,
            status: "Done".into(),
            status_category: "done".into(),
            parent_key: None,
            epic_key: Some("A-100".into()),
            assignee: None,
            assignee_id: None,
            original_estimate_s: Some(7200),
            time_spent_s: Some(9000),
            labels: vec![],
            components: vec![],
            updated: None,
        }
    }

    fn worklog(id: &str, key: &str, author: &str, date: &str, hour: u32, seconds: i64) -> Worklog {
        let started =
            DateTime::parse_from_rfc3339(&format!("{date}T{hour:02}:00:00+01:00")).unwrap();
        Worklog {
            id: id.into(),
            issue_key: key.into(),
            author_id: author.into(),
            author_name: None,
            started,
            date: started.date_naive(),
            seconds,
            comment: String::new(),
            updated: None,
        }
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn targets_scale_with_weekly_hours() {
        let work = WorkSettings::default(); // 40 h, Mon-Fri
        let monday = day("2026-10-05");
        let sunday = day("2026-10-11");
        assert_eq!(
            member_target_seconds(&member("a", None), monday, &work),
            8 * 3600
        );
        assert_eq!(
            member_target_seconds(&member("a", Some(20.0)), monday, &work),
            4 * 3600
        );
        assert_eq!(
            member_target_seconds(&member("a", Some(20.0)), sunday, &work),
            0
        );
    }

    #[test]
    fn aggregates_team_members_days_and_issues() {
        let work = WorkSettings::default();
        let members = vec![
            member("alice", None),
            member("bob", Some(20.0)),
            member("carl", None),
        ];
        let mut issues = HashMap::new();
        issues.insert("A-1".to_string(), issue("A-1", "A", "Bug"));
        issues.insert("A-2".to_string(), issue("A-2", "A", "Story"));
        issues.insert("B-1".to_string(), issue("B-1", "B", "Task"));
        let mut projects = HashMap::new();
        projects.insert(
            "A".to_string(),
            Project {
                id: "1".into(),
                key: "A".into(),
                name: "Alpha".into(),
                avatar_url: None,
                color: Some("#111".into()),
            },
        );
        // Mon 2026-10-05 .. Fri 2026-10-09; today is Wednesday
        let from = day("2026-10-05");
        let to = day("2026-10-09");
        let today = day("2026-10-07");
        let worklogs = vec![
            worklog("1", "A-1", "alice", "2026-10-05", 9, 3600),
            worklog("2", "A-1", "alice", "2026-10-05", 14, 1800),
            worklog("3", "A-2", "alice", "2026-10-06", 9, 7200),
            worklog("4", "B-1", "bob", "2026-10-06", 10, 3600),
            worklog("5", "B-1", "bob", "2026-10-07", 10, 3600),
            worklog("6", "Z-9", "bob", "2026-10-07", 11, 600), // unknown issue -> project Z
            worklog("7", "A-1", "stranger", "2026-10-07", 11, 99999), // not a member
            worklog("8", "A-1", "alice", "2026-10-12", 11, 99999), // out of range
        ];
        let stats = compute(
            &worklogs, &issues, &projects, &members, from, to, &work, today,
        );
        assert_eq!(stats.total_seconds, 3600 + 1800 + 7200 + 3600 + 3600 + 600);
        assert_eq!(stats.worklog_count, 6);
        assert_eq!(stats.issue_count, 4);
        assert_eq!(stats.days_logged, 3);
        assert_eq!(stats.days.len(), 5);
        // targets: 3 days up to today; alice 8h, bob 4h, carl 8h per day
        assert_eq!(stats.target_seconds, 3 * (8 + 4 + 8) * 3600);
        assert_eq!(stats.work_days, 3);
        assert_eq!(stats.days[3].target_seconds, 0); // Thursday is in the future
        assert_eq!(stats.days[0].by_member["alice"], 5400);

        let alice = &stats.members[0];
        assert_eq!(alice.seconds, 12600);
        assert_eq!(alice.target_seconds, 3 * 8 * 3600);
        assert_eq!(alice.issue_count, 2);
        assert_eq!(alice.days_logged, 2);
        assert_eq!(alice.worklog_count, 3);
        assert_eq!(alice.avg_per_logged_day_seconds, 6300);
        assert_eq!(
            alice.last_worklog.unwrap().to_rfc3339(),
            "2026-10-06T08:00:00+00:00"
        );
        assert_eq!(alice.by_project[0].label, "Alpha");
        let bob = &stats.members[1];
        assert_eq!(bob.target_seconds, 3 * 4 * 3600);
        assert_eq!(bob.by_project.len(), 2);
        let carl = &stats.members[2];
        assert_eq!(carl.seconds, 0);
        assert_eq!(carl.avg_per_logged_day_seconds, 0);
        assert!(carl.last_worklog.is_none());

        assert_eq!(stats.by_project[0].key, "A");
        assert_eq!(stats.by_project[0].color.as_deref(), Some("#111"));
        assert_eq!(stats.by_project[1].key, "B");
        assert_eq!(stats.by_project[2].key, "Z");
        assert_eq!(stats.by_issue[0].key, "A-2");
        assert_eq!(stats.by_issue[1].key, "B-1");
        assert_eq!(stats.by_issue[1].by_member["bob"], 7200);
        assert_eq!(stats.by_issue[1].worklog_count, 2);
        assert_eq!(stats.by_issue[0].epic_key.as_deref(), Some("A-100"));
        assert_eq!(stats.by_issue[0].time_spent_s, Some(9000));
        let unknown = stats.by_issue.iter().find(|i| i.key == "Z-9").unwrap();
        assert_eq!(unknown.project_key, "Z");
        assert_eq!(unknown.status_category, "undefined");
        assert_eq!(stats.by_issue_type[0].key, "Story");
        assert_eq!(stats.by_weekday[0], 5400);
        assert_eq!(stats.by_weekday[1], 10800);
        assert_eq!(stats.by_weekday[6], 0);
    }
}
