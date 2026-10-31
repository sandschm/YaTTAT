//! Loads the worklogs of team members for a date range into the local cache.
//!
//! Two sources:
//! * Jira: one JQL search (`worklogAuthor in (...) AND worklogDate ...`) finds the issues that
//!   received worklogs, then every issue's worklog list is read and filtered by author and date.
//! * Tempo Cloud: the worklogs of every member are read from the Tempo API (which knows the real
//!   author even when the Jira worklog belongs to the Tempo app), then the referenced issues are
//!   resolved through Jira.
//!
//! Days that are already cached for an author are skipped unless `force` is set; worklogs that
//! disappeared from the source are removed from the cache for the fetched range.

use crate::error::Result;
use crate::jira::{jql_quote, JiraClient};
use crate::models::{Issue, Worklog};
use crate::storage::Store;
use crate::tempo::TempoCloud;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

const PARALLEL_REQUESTS: usize = 4;
const MAX_ISSUES: usize = 5000;
const MAX_OTHER_AUTHORS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FetchRequest {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub author_ids: Vec<String>,
    /// Restrict to these projects (empty = all).
    pub project_keys: Vec<String>,
    /// Re-fetch even if the range is cached.
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct FetchProgress {
    /// "issues" while searching or resolving issues, "worklogs" while reading worklogs.
    pub phase: String,
    pub done: usize,
    pub total: usize,
}

/// Worklogs in the fetched range whose author is not a requested member (Jira source). Shows
/// e.g. that Jira attributes the team's worklogs to the Tempo app.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthorCount {
    pub id: String,
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FetchOutcome {
    /// `false` when everything was cached already and nothing was requested.
    pub fetched: bool,
    /// "jira" or "tempoCloud"
    pub source: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub authors: usize,
    /// Requested authors without any worklog in the fetched range (permission problem, wrong
    /// source or simply no bookings).
    #[serde(default)]
    pub authors_without_worklogs: Vec<String>,
    pub issues: usize,
    pub worklogs: usize,
    pub removed: usize,
    #[serde(default)]
    pub other_authors: Vec<AuthorCount>,
    /// Tempo worklogs whose issue could not be read from Jira (no permission or deleted).
    #[serde(default)]
    pub unresolved_issues: usize,
}

impl FetchOutcome {
    fn nothing(req: &FetchRequest, source: &str) -> Self {
        FetchOutcome {
            fetched: false,
            source: source.into(),
            from: req.from,
            to: req.to,
            authors: 0,
            authors_without_worklogs: vec![],
            issues: 0,
            worklogs: 0,
            removed: 0,
            other_authors: vec![],
            unresolved_issues: 0,
        }
    }
}

pub fn worklog_jql(
    author_ids: &[String],
    from: NaiveDate,
    to: NaiveDate,
    project_keys: &[String],
) -> String {
    let authors = author_ids
        .iter()
        .map(|a| jql_quote(a))
        .collect::<Vec<_>>()
        .join(", ");
    let mut clauses = vec![
        format!("worklogAuthor in ({authors})"),
        format!("worklogDate >= \"{}\"", from.format("%Y-%m-%d")),
        format!("worklogDate <= \"{}\"", to.format("%Y-%m-%d")),
    ];
    if !project_keys.is_empty() {
        let keys = project_keys
            .iter()
            .map(|k| jql_quote(k))
            .collect::<Vec<_>>()
            .join(", ");
        clauses.push(format!("project in ({keys})"));
    }
    format!("{} ORDER BY updated DESC", clauses.join(" AND "))
}

/// The range and authors that actually need a request, or `None` when the cache is complete.
fn plan(store: &Store, req: &FetchRequest) -> Result<Option<(NaiveDate, NaiveDate, Vec<String>)>> {
    if req.author_ids.is_empty() || req.to < req.from {
        return Ok(None);
    }
    if req.force {
        return Ok(Some((req.from, req.to, req.author_ids.clone())));
    }
    let mut lo: Option<NaiveDate> = None;
    let mut hi: Option<NaiveDate> = None;
    let mut authors = Vec::new();
    for a in &req.author_ids {
        let missing = store.missing_days(a, req.from, req.to)?;
        if let (Some(first), Some(last)) = (missing.first(), missing.last()) {
            lo = Some(lo.map_or(*first, |l| l.min(*first)));
            hi = Some(hi.map_or(*last, |h| h.max(*last)));
            authors.push(a.clone());
        }
    }
    Ok(lo.zip(hi).map(|(l, h)| (l, h, authors)))
}

struct Collected {
    issues: usize,
    worklogs: Vec<Worklog>,
    other_authors: Vec<AuthorCount>,
    unresolved_issues: usize,
}

/// What one fetch covers: the (possibly narrowed) range and the authors that need it.
struct Scope<'a> {
    from: NaiveDate,
    to: NaiveDate,
    authors: &'a [String],
    project_keys: &'a [String],
    now: DateTime<Utc>,
}

/// Jira source: JQL for the issues, then the worklogs of every issue.
async fn collect_from_jira(
    jira: &JiraClient,
    store: &Store,
    scope: &Scope<'_>,
    progress: &(dyn Fn(FetchProgress) + Sync),
) -> Result<Collected> {
    let Scope {
        from,
        to,
        authors,
        project_keys,
        now,
    } = *scope;
    progress(FetchProgress {
        phase: "issues".into(),
        done: 0,
        total: 0,
    });
    // One day of slack on both sides: JQL evaluates worklogDate in the time zone of the searching
    // user, the cache attributes a worklog to the author's day.
    let jql = worklog_jql(
        authors,
        from - Duration::days(1),
        to + Duration::days(1),
        project_keys,
    );
    tracing::debug!("worklog jql: {jql}");
    let issues = jira.search_issues(&jql, MAX_ISSUES).await?;
    remember_issues(store, &issues, now)?;
    let total = issues.len();
    progress(FetchProgress {
        phase: "worklogs".into(),
        done: 0,
        total,
    });
    let author_set: HashSet<&str> = authors.iter().map(String::as_str).collect();
    let mut collected: Vec<Worklog> = Vec::new();
    let mut others: HashMap<String, (String, usize)> = HashMap::new();
    let mut done = 0usize;
    // Owned keys: a closure over `&Issue` makes the future's `Send` bound unprovable.
    let keys: Vec<String> = issues.iter().map(|i| i.key.clone()).collect();
    let mut results = stream::iter(keys)
        .map(|key: String| async move { jira.issue_worklogs(&key).await })
        .buffer_unordered(PARALLEL_REQUESTS);
    while let Some(res) = results.next().await {
        for w in res? {
            if w.date < from || w.date > to {
                continue;
            }
            if author_set.contains(w.author_id.as_str()) {
                collected.push(w);
            } else {
                let entry = others
                    .entry(w.author_id.clone())
                    .or_insert_with(|| (w.author_name.clone().unwrap_or_default(), 0));
                entry.1 += 1;
            }
        }
        done += 1;
        progress(FetchProgress {
            phase: "worklogs".into(),
            done,
            total,
        });
    }
    let mut other_authors: Vec<AuthorCount> = others
        .into_iter()
        .map(|(id, (name, count))| AuthorCount { id, name, count })
        .collect();
    other_authors.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.id.cmp(&b.id)));
    other_authors.truncate(MAX_OTHER_AUTHORS);
    Ok(Collected {
        issues: total,
        worklogs: collected,
        other_authors,
        unresolved_issues: 0,
    })
}

/// Tempo source: worklogs per member from Tempo, issues resolved through Jira by id.
async fn collect_from_tempo(
    jira: &JiraClient,
    tempo: &TempoCloud,
    store: &Store,
    scope: &Scope<'_>,
    progress: &(dyn Fn(FetchProgress) + Sync),
) -> Result<Collected> {
    let Scope {
        from,
        to,
        authors,
        project_keys,
        now,
    } = *scope;
    progress(FetchProgress {
        phase: "worklogs".into(),
        done: 0,
        total: authors.len(),
    });
    let mut raw = Vec::new();
    for (i, author) in authors.iter().enumerate() {
        let logs = tempo.user_worklogs(author, from, to).await?;
        raw.extend(
            logs.into_iter()
                .filter(|w| w.author_id == *author && w.start_date >= from && w.start_date <= to),
        );
        progress(FetchProgress {
            phase: "worklogs".into(),
            done: i + 1,
            total: authors.len(),
        });
    }
    let mut ids: Vec<i64> = raw.iter().map(|w| w.issue_id).collect();
    ids.sort_unstable();
    ids.dedup();
    let total = ids.len();
    progress(FetchProgress {
        phase: "issues".into(),
        done: 0,
        total,
    });
    let mut resolved: HashMap<i64, Issue> = HashMap::new();
    let mut done = 0usize;
    let mut results = stream::iter(ids)
        .map(|id: i64| async move { jira.get_issue_opt(&id.to_string()).await.map(|i| (id, i)) })
        .buffer_unordered(PARALLEL_REQUESTS);
    while let Some(res) = results.next().await {
        let (id, issue) = res?;
        if let Some(issue) = issue {
            resolved.insert(id, issue);
        }
        done += 1;
        progress(FetchProgress {
            phase: "issues".into(),
            done,
            total,
        });
    }
    let issues: Vec<Issue> = resolved.values().cloned().collect();
    remember_issues(store, &issues, now)?;
    let allowed: HashSet<&str> = project_keys.iter().map(String::as_str).collect();
    let mut unresolved = 0usize;
    let mut worklogs = Vec::new();
    for w in raw {
        match resolved.get(&w.issue_id) {
            Some(issue) => {
                if allowed.is_empty() || allowed.contains(issue.project_key.as_str()) {
                    worklogs.push(w.into_worklog(&issue.key));
                }
            }
            None => unresolved += 1,
        }
    }
    Ok(Collected {
        issues: issues.len(),
        worklogs,
        other_authors: vec![],
        unresolved_issues: unresolved,
    })
}

fn remember_issues(store: &Store, issues: &[Issue], now: DateTime<Utc>) -> Result<()> {
    store.upsert_issues(issues, now)?;
    let mut names: Vec<(String, String)> = issues
        .iter()
        .filter_map(|i| i.project_name.clone().map(|n| (i.project_key.clone(), n)))
        .collect();
    names.sort();
    names.dedup();
    store.upsert_project_names(&names)
}

/// Fetches the missing part of `req` from Jira (or Tempo when a client is given) into the cache.
pub async fn fetch_range(
    jira: &JiraClient,
    tempo: Option<&TempoCloud>,
    store: &Store,
    req: &FetchRequest,
    now: DateTime<Utc>,
    progress: &(dyn Fn(FetchProgress) + Sync),
) -> Result<FetchOutcome> {
    let source = if tempo.is_some() {
        "tempoCloud"
    } else {
        "jira"
    };
    let Some((from, to, authors)) = plan(store, req)? else {
        return Ok(FetchOutcome::nothing(req, source));
    };
    let scope = Scope {
        from,
        to,
        authors: &authors,
        project_keys: &req.project_keys,
        now,
    };
    let collected = match tempo {
        Some(t) => collect_from_tempo(jira, t, store, &scope, progress).await?,
        None => collect_from_jira(jira, store, &scope, progress).await?,
    };
    store.upsert_worklogs(&collected.worklogs, now)?;
    let keep: HashSet<String> = collected.worklogs.iter().map(|w| w.id.clone()).collect();
    let removed = store.prune_worklogs(&authors, from, to, &keep)?;
    store.mark_coverage(&authors, from, to, now)?;
    let with_worklogs: HashSet<&str> = collected
        .worklogs
        .iter()
        .map(|w| w.author_id.as_str())
        .collect();
    let authors_without_worklogs: Vec<String> = authors
        .iter()
        .filter(|a| !with_worklogs.contains(a.as_str()))
        .cloned()
        .collect();
    Ok(FetchOutcome {
        fetched: true,
        source: source.into(),
        from,
        to,
        authors: authors.len(),
        authors_without_worklogs,
        issues: collected.issues,
        worklogs: collected.worklogs.len(),
        removed,
        other_authors: collected.other_authors,
        unresolved_issues: collected.unresolved_issues,
    })
}

// ------------------------------------------------------------------ comparison

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DayDiff {
    pub date: NaiveDate,
    pub jira_seconds: i64,
    pub tempo_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemberComparison {
    pub id: String,
    pub jira_seconds: i64,
    pub tempo_seconds: i64,
    pub jira_worklogs: usize,
    pub tempo_worklogs: usize,
    /// Days where the two sources disagree (at most [`MAX_DIFF_DAYS`]).
    pub differing_days: Vec<DayDiff>,
}

/// Result of reading the same range from Jira and Tempo. The cache is not changed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceComparison {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub jira_seconds: i64,
    pub tempo_seconds: i64,
    pub jira_issues: usize,
    pub tempo_issues: usize,
    pub members: Vec<MemberComparison>,
    pub jira_other_authors: Vec<AuthorCount>,
    pub tempo_unresolved_issues: usize,
}

const MAX_DIFF_DAYS: usize = 62;

type Sums<'a> = HashMap<&'a str, (i64, usize, BTreeMap<NaiveDate, i64>)>;

fn sums(worklogs: &[Worklog]) -> Sums<'_> {
    let mut out: Sums<'_> = HashMap::new();
    for w in worklogs {
        let e = out
            .entry(w.author_id.as_str())
            .or_insert_with(|| (0, 0, BTreeMap::new()));
        e.0 += w.seconds;
        e.1 += 1;
        *e.2.entry(w.date).or_insert(0) += w.seconds;
    }
    out
}

/// Reads the whole range from both sources (ignoring the cache's coverage) and compares them per
/// member and day. Issues seen on the way are remembered, worklogs are not written.
pub async fn compare_sources(
    jira: &JiraClient,
    tempo: &TempoCloud,
    store: &Store,
    req: &FetchRequest,
    now: DateTime<Utc>,
    progress: &(dyn Fn(&str, FetchProgress) + Sync),
) -> Result<SourceComparison> {
    let scope = Scope {
        from: req.from,
        to: req.to,
        authors: &req.author_ids,
        project_keys: &req.project_keys,
        now,
    };
    let j = collect_from_jira(jira, store, &scope, &|p| progress("jira", p)).await?;
    let t = collect_from_tempo(jira, tempo, store, &scope, &|p| progress("tempo", p)).await?;
    let js = sums(&j.worklogs);
    let ts = sums(&t.worklogs);
    let empty = (0, 0, BTreeMap::new());
    let members = req
        .author_ids
        .iter()
        .map(|id| {
            let a = js.get(id.as_str()).unwrap_or(&empty);
            let b = ts.get(id.as_str()).unwrap_or(&empty);
            let days: BTreeMap<NaiveDate, (i64, i64)> =
                a.2.iter()
                    .map(|(d, s)| (*d, (*s, 0)))
                    .chain(b.2.iter().map(|(d, s)| (*d, (0, *s))))
                    .fold(BTreeMap::new(), |mut acc, (d, (x, y))| {
                        let e = acc.entry(d).or_insert((0, 0));
                        e.0 += x;
                        e.1 += y;
                        acc
                    });
            let mut differing_days: Vec<DayDiff> = days
                .into_iter()
                .filter(|(_, (x, y))| x != y)
                .map(|(date, (jira_seconds, tempo_seconds))| DayDiff {
                    date,
                    jira_seconds,
                    tempo_seconds,
                })
                .collect();
            differing_days.truncate(MAX_DIFF_DAYS);
            MemberComparison {
                id: id.clone(),
                jira_seconds: a.0,
                tempo_seconds: b.0,
                jira_worklogs: a.1,
                tempo_worklogs: b.1,
                differing_days,
            }
        })
        .collect::<Vec<_>>();
    Ok(SourceComparison {
        from: req.from,
        to: req.to,
        jira_seconds: members.iter().map(|m| m.jira_seconds).sum(),
        tempo_seconds: members.iter().map(|m| m.tempo_seconds).sum(),
        jira_issues: j.issues,
        tempo_issues: t.issues,
        members,
        jira_other_authors: j.other_authors,
        tempo_unresolved_issues: t.unresolved_issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Credential, Deployment};
    use serde_json::json;
    use std::sync::Mutex;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn jql_lists_authors_dates_and_projects() {
        let jql = worklog_jql(
            &["alice".into(), "712020:ab\"c".into()],
            day("2026-10-01"),
            day("2026-10-31"),
            &["A".into()],
        );
        assert_eq!(
            jql,
            "worklogAuthor in (\"alice\", \"712020:ab\\\"c\") AND worklogDate >= \"2026-10-01\" AND worklogDate <= \"2026-10-31\" AND project in (\"A\") ORDER BY updated DESC"
        );
        assert!(
            !worklog_jql(&["a".into()], day("2026-10-01"), day("2026-10-02"), &[])
                .contains("project in")
        );
    }

    async fn mock_jira(server: &MockServer, worklogs_a1: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path("/rest/api/2/field"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total": 2, "startAt": 0,
                "issues": [
                    {"id": "1", "key": "A-1", "fields": {"summary": "One", "project": {"key": "A", "name": "Alpha"}, "issuetype": {"name": "Task"}, "status": {"name": "Open", "statusCategory": {"key": "new"}}}},
                    {"id": "2", "key": "A-2", "fields": {"summary": "Two", "project": {"key": "A"}, "issuetype": {"name": "Bug"}, "status": {"name": "Open", "statusCategory": {"key": "new"}}}}
                ]
            })))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/issue/A-1/worklog"))
            .respond_with(ResponseTemplate::new(200).set_body_json(worklogs_a1))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/issue/A-2/worklog"))
            .and(query_param("startAt", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total": 1, "startAt": 0, "maxResults": 1000,
                "worklogs": [
                    {"id": "20", "author": {"name": "bob"}, "started": "2026-10-02T10:00:00.000+0200", "timeSpentSeconds": 600}
                ]
            })))
            .mount(server)
            .await;
    }

    fn logs_a1(include_second: bool) -> serde_json::Value {
        let mut logs = vec![
            json!({"id": "10", "author": {"name": "alice"}, "started": "2026-10-01T09:00:00.000+0200", "timeSpentSeconds": 3600, "comment": "x"}),
            json!({"id": "12", "author": {"name": "alice"}, "started": "2026-09-30T23:30:00.000+0200", "timeSpentSeconds": 60}), // before range
            json!({"id": "13", "author": {"name": "zed", "displayName": "Tempo Timesheets"}, "started": "2026-10-01T09:00:00.000+0200", "timeSpentSeconds": 60}), // not requested
            json!({"id": "14", "author": {"name": "zed", "displayName": "Tempo Timesheets"}, "started": "2026-10-02T09:00:00.000+0200", "timeSpentSeconds": 60}),
        ];
        if include_second {
            logs.push(json!({"id": "11", "author": {"name": "alice"}, "started": "2026-10-03T09:00:00.000+0200", "timeSpentSeconds": 1800}));
        }
        json!({"total": logs.len(), "startAt": 0, "maxResults": 1000, "worklogs": logs})
    }

    #[tokio::test]
    async fn fetches_filters_caches_and_prunes() {
        let server = MockServer::start().await;
        mock_jira(&server, logs_a1(true)).await;
        let jira = JiraClient::new(
            &server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::DataCenter,
        )
        .unwrap();
        let store = Store::open_in_memory().unwrap();
        let seen: Mutex<Vec<FetchProgress>> = Mutex::new(vec![]);
        let progress = |p: FetchProgress| seen.lock().unwrap().push(p);
        let req = FetchRequest {
            from: day("2026-10-01"),
            to: day("2026-10-05"),
            author_ids: vec!["alice".into(), "bob".into()],
            project_keys: vec![],
            force: false,
        };
        let out = fetch_range(&jira, None, &store, &req, Utc::now(), &progress)
            .await
            .unwrap();
        assert!(out.fetched);
        assert_eq!(out.source, "jira");
        assert_eq!(out.issues, 2);
        assert_eq!(out.worklogs, 3);
        assert_eq!(out.removed, 0);
        assert_eq!(out.authors, 2);
        assert!(out.authors_without_worklogs.is_empty());
        assert_eq!(
            out.other_authors,
            vec![AuthorCount {
                id: "zed".into(),
                name: "Tempo Timesheets".into(),
                count: 2
            }]
        );
        {
            let p = seen.lock().unwrap();
            assert_eq!(p[0].phase, "issues");
            assert_eq!(p.last().unwrap().done, 2);
        }
        let cached = store
            .list_worklogs(
                day("2026-09-01"),
                day("2026-10-31"),
                &["alice".into(), "bob".into()],
            )
            .unwrap();
        assert_eq!(
            cached.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(),
            vec!["10", "20", "11"]
        );
        assert!(store.get_issue("A-2").unwrap().is_some());
        assert_eq!(store.list_projects().unwrap()[0].name, "Alpha");
        assert!(store
            .coverage(&req.author_ids, req.from, req.to)
            .unwrap()
            .complete());

        // Cached: a second call does not touch Jira.
        let received_before = server.received_requests().await.unwrap().len();
        let out = fetch_range(&jira, None, &store, &req, Utc::now(), &progress)
            .await
            .unwrap();
        assert!(!out.fetched);
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            received_before
        );

        // Extending the range only fetches the missing days (plan picks the gap).
        let wider = FetchRequest {
            to: day("2026-10-07"),
            ..req.clone()
        };
        let out = fetch_range(&jira, None, &store, &wider, Utc::now(), &progress)
            .await
            .unwrap();
        assert!(out.fetched);
        assert_eq!(out.from, day("2026-10-06"));
        assert_eq!(out.to, day("2026-10-07"));

        // Force with a worklog gone from Jira: it is pruned from the cache.
        server.reset().await;
        mock_jira(&server, logs_a1(false)).await;
        let forced = FetchRequest {
            force: true,
            ..req.clone()
        };
        let out = fetch_range(&jira, None, &store, &forced, Utc::now(), &progress)
            .await
            .unwrap();
        assert_eq!(out.removed, 1);
        let cached = store
            .list_worklogs(day("2026-09-01"), day("2026-10-31"), &["alice".into()])
            .unwrap();
        assert_eq!(cached.len(), 1);
    }

    #[tokio::test]
    async fn tempo_source_resolves_issues_and_filters_projects() {
        let jira_server = MockServer::start().await;
        let tempo_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/field"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&jira_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/issue/10001"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "10001", "key": "A-1", "fields": {"summary": "One", "project": {"key": "A", "name": "Alpha"}, "issuetype": {"name": "Task"}, "status": {"name": "Open", "statusCategory": {"key": "new"}}}})))
            .mount(&jira_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/issue/10002"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "10002", "key": "B-7", "fields": {"summary": "Other project", "project": {"key": "B"}, "issuetype": {"name": "Task"}, "status": {"name": "Open", "statusCategory": {"key": "new"}}}})))
            .mount(&jira_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/issue/10003"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json!({"errorMessages": ["Issue does not exist"]})),
            )
            .mount(&jira_server)
            .await;
        let tlog = |id: i64, issue: i64, author: &str, date: &str| json!({"tempoWorklogId": id, "issue": {"id": issue}, "timeSpentSeconds": 3600, "startDate": date, "startTime": "09:00:00", "description": "d", "author": {"accountId": author}});
        Mock::given(method("GET"))
            .and(path("/worklogs/user/alice"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"metadata": {"count": 4}, "results": [
                    tlog(1, 10001, "alice", "2026-10-01"),
                    tlog(2, 10002, "alice", "2026-10-02"), // project B: filtered out
                    tlog(3, 10003, "alice", "2026-10-02"), // unknown issue
                    tlog(4, 10001, "alice", "2026-10-09") // after range
                ]}),
            ))
            .mount(&tempo_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/worklogs/user/bob"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"metadata": {"count": 1}, "results": [
                    tlog(5, 10001, "bob", "2026-10-03")
                ]}),
            ))
            .mount(&tempo_server)
            .await;
        let jira = JiraClient::new(
            &jira_server.uri(),
            Credential::ApiToken {
                email: "e".into(),
                token: "t".into(),
            },
            Deployment::Cloud,
        )
        .unwrap();
        let tempo = TempoCloud::new(&tempo_server.uri(), "tok").unwrap();
        let store = Store::open_in_memory().unwrap();
        let req = FetchRequest {
            from: day("2026-10-01"),
            to: day("2026-10-05"),
            author_ids: vec!["alice".into(), "bob".into()],
            project_keys: vec!["A".into()],
            force: false,
        };
        let out = fetch_range(&jira, Some(&tempo), &store, &req, Utc::now(), &|_| {})
            .await
            .unwrap();
        assert!(out.fetched);
        assert_eq!(out.source, "tempoCloud");
        assert_eq!(out.authors, 2);
        assert!(out.authors_without_worklogs.is_empty());
        assert_eq!(out.issues, 2);
        assert_eq!(out.worklogs, 2);
        assert_eq!(out.unresolved_issues, 1);
        let cached = store
            .list_worklogs(day("2026-10-01"), day("2026-10-31"), &req.author_ids)
            .unwrap();
        assert_eq!(
            cached.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(),
            vec!["tempo-1", "tempo-5"]
        );
        assert_eq!(cached[0].issue_key, "A-1");
        assert_eq!(cached[0].comment, "d");
        assert_eq!(store.list_projects().unwrap()[0].name, "Alpha");
        assert!(store
            .coverage(&req.author_ids, req.from, req.to)
            .unwrap()
            .complete());
    }

    #[tokio::test]
    async fn comparison_reports_differences_per_member_and_day() {
        let jira_server = MockServer::start().await;
        let tempo_server = MockServer::start().await;
        // Jira: alice has 1 h on A-1 on 10-01 and 10-02; bob nothing; the Tempo app logged for bob.
        mock_jira(
            &jira_server,
            json!({"total": 3, "startAt": 0, "maxResults": 1000, "worklogs": [
                {"id": "10", "author": {"name": "alice"}, "started": "2026-10-01T09:00:00.000+0200", "timeSpentSeconds": 3600},
                {"id": "11", "author": {"name": "alice"}, "started": "2026-10-02T09:00:00.000+0200", "timeSpentSeconds": 3600},
                {"id": "13", "author": {"name": "app", "displayName": "Tempo Timesheets"}, "started": "2026-10-02T09:00:00.000+0200", "timeSpentSeconds": 7200}
            ]}),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/issue/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "1", "key": "A-1", "fields": {"summary": "One", "project": {"key": "A"}, "issuetype": {"name": "Task"}, "status": {"name": "Open", "statusCategory": {"key": "new"}}}})))
            .mount(&jira_server)
            .await;
        let tlog = |id: i64, author: &str, date: &str, secs: i64| json!({"tempoWorklogId": id, "issue": {"id": 1}, "timeSpentSeconds": secs, "startDate": date, "author": {"accountId": author}});
        Mock::given(method("GET"))
            .and(path("/worklogs/user/alice"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": [
                tlog(1, "alice", "2026-10-01", 3600),
                tlog(2, "alice", "2026-10-02", 1800) // Jira says 1 h
            ]})))
            .mount(&tempo_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/worklogs/user/bob"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": [
                tlog(3, "bob", "2026-10-02", 7200)
            ]})))
            .mount(&tempo_server)
            .await;
        let jira = JiraClient::new(
            &jira_server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::DataCenter,
        )
        .unwrap();
        let tempo = TempoCloud::new(&tempo_server.uri(), "tok").unwrap();
        let store = Store::open_in_memory().unwrap();
        let req = FetchRequest {
            from: day("2026-10-01"),
            to: day("2026-10-05"),
            author_ids: vec!["alice".into(), "bob".into()],
            project_keys: vec![],
            force: true,
        };
        let seen = Mutex::new(Vec::<String>::new());
        let progress = |source: &str, p: FetchProgress| {
            seen.lock().unwrap().push(format!("{source}:{}", p.phase))
        };
        let cmp = compare_sources(&jira, &tempo, &store, &req, Utc::now(), &progress)
            .await
            .unwrap();
        assert_eq!(cmp.jira_seconds, 7200 + 600); // alice 2 h + bob's Jira worklog on A-2 (10 min)
        assert_eq!(cmp.tempo_seconds, 3600 + 1800 + 7200);
        assert_eq!(cmp.members[0].id, "alice");
        assert_eq!(cmp.members[0].jira_worklogs, 2);
        assert_eq!(cmp.members[0].tempo_worklogs, 2);
        assert_eq!(
            cmp.members[0].differing_days,
            vec![DayDiff {
                date: day("2026-10-02"),
                jira_seconds: 3600,
                tempo_seconds: 1800
            }]
        );
        assert_eq!(cmp.members[1].jira_seconds, 600);
        assert_eq!(cmp.members[1].tempo_seconds, 7200);
        assert_eq!(cmp.members[1].differing_days.len(), 1);
        assert_eq!(cmp.jira_other_authors[0].name, "Tempo Timesheets");
        assert_eq!(cmp.tempo_unresolved_issues, 0);
        // the cache holds the issues but no worklogs
        assert!(store.get_issue("A-1").unwrap().is_some());
        assert!(store
            .list_worklogs(day("2026-10-01"), day("2026-10-05"), &req.author_ids)
            .unwrap()
            .is_empty());
        let seen = seen.lock().unwrap();
        assert!(seen.iter().any(|s| s == "jira:worklogs"));
        assert!(seen.iter().any(|s| s == "tempo:issues"));
    }

    #[tokio::test]
    async fn empty_requests_do_nothing() {
        let server = MockServer::start().await;
        let jira = JiraClient::new(
            &server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::DataCenter,
        )
        .unwrap();
        let store = Store::open_in_memory().unwrap();
        let req = FetchRequest {
            from: day("2026-10-05"),
            to: day("2026-10-01"),
            author_ids: vec!["a".into()],
            project_keys: vec![],
            force: true,
        };
        let out = fetch_range(&jira, None, &store, &req, Utc::now(), &|_| {})
            .await
            .unwrap();
        assert!(!out.fetched);
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
