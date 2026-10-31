//! Loads the worklogs of team members for a date range from Jira into the local cache.
//!
//! Strategy: one JQL search (`worklogAuthor in (...) AND worklogDate ...`) finds the issues that
//! received worklogs, then every issue's worklog list is read and filtered by author and date.
//! Days that are already cached for an author are skipped unless `force` is set; worklogs that
//! disappeared from Jira are removed from the cache for the fetched range.

use crate::error::Result;
use crate::jira::{jql_quote, JiraClient};
use crate::models::Worklog;
use crate::storage::Store;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const PARALLEL_ISSUE_REQUESTS: usize = 4;
const MAX_ISSUES: usize = 5000;

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
    /// "issues" while searching, "worklogs" while reading per-issue worklogs.
    pub phase: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FetchOutcome {
    /// `false` when everything was cached already and nothing was requested from Jira.
    pub fetched: bool,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub authors: usize,
    pub issues: usize,
    pub worklogs: usize,
    pub removed: usize,
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

pub async fn fetch_range(
    jira: &JiraClient,
    store: &Store,
    req: &FetchRequest,
    now: DateTime<Utc>,
    progress: &(dyn Fn(FetchProgress) + Sync),
) -> Result<FetchOutcome> {
    let Some((from, to, authors)) = plan(store, req)? else {
        return Ok(FetchOutcome {
            fetched: false,
            from: req.from,
            to: req.to,
            authors: 0,
            issues: 0,
            worklogs: 0,
            removed: 0,
        });
    };
    progress(FetchProgress {
        phase: "issues".into(),
        done: 0,
        total: 0,
    });
    // One day of slack on both sides: JQL evaluates worklogDate in the time zone of the searching
    // user, the cache attributes a worklog to the author's day.
    let jql = worklog_jql(
        &authors,
        from - Duration::days(1),
        to + Duration::days(1),
        &req.project_keys,
    );
    tracing::debug!("worklog jql: {jql}");
    let issues = jira.search_issues(&jql, MAX_ISSUES).await?;
    store.upsert_issues(&issues, now)?;
    let mut names: Vec<(String, String)> = issues
        .iter()
        .filter_map(|i| i.project_name.clone().map(|n| (i.project_key.clone(), n)))
        .collect();
    names.sort();
    names.dedup();
    store.upsert_project_names(&names)?;
    let total = issues.len();
    progress(FetchProgress {
        phase: "worklogs".into(),
        done: 0,
        total,
    });
    let author_set: HashSet<&str> = authors.iter().map(String::as_str).collect();
    let mut collected: Vec<Worklog> = Vec::new();
    let mut done = 0usize;
    // Owned keys: a closure over `&Issue` makes the future's `Send` bound unprovable
    // ("implementation of FnOnce is not general enough").
    let keys: Vec<String> = issues.iter().map(|i| i.key.clone()).collect();
    let mut results = stream::iter(keys)
        .map(|key: String| async move { jira.issue_worklogs(&key).await })
        .buffer_unordered(PARALLEL_ISSUE_REQUESTS);
    while let Some(res) = results.next().await {
        let logs = res?;
        collected.extend(logs.into_iter().filter(|w| {
            author_set.contains(w.author_id.as_str()) && w.date >= from && w.date <= to
        }));
        done += 1;
        progress(FetchProgress {
            phase: "worklogs".into(),
            done,
            total,
        });
    }
    store.upsert_worklogs(&collected, now)?;
    let keep: HashSet<String> = collected.iter().map(|w| w.id.clone()).collect();
    let removed = store.prune_worklogs(&authors, from, to, &keep)?;
    store.mark_coverage(&authors, from, to, now)?;
    Ok(FetchOutcome {
        fetched: true,
        from,
        to,
        authors: authors.len(),
        issues: total,
        worklogs: collected.len(),
        removed,
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
            json!({"id": "13", "author": {"name": "zed"}, "started": "2026-10-01T09:00:00.000+0200", "timeSpentSeconds": 60}), // not requested
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
        let out = fetch_range(&jira, &store, &req, Utc::now(), &progress)
            .await
            .unwrap();
        assert!(out.fetched);
        assert_eq!(out.issues, 2);
        assert_eq!(out.worklogs, 3);
        assert_eq!(out.removed, 0);
        assert_eq!(out.authors, 2);
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
        let out = fetch_range(&jira, &store, &req, Utc::now(), &progress)
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
        let out = fetch_range(&jira, &store, &wider, Utc::now(), &progress)
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
        let out = fetch_range(&jira, &store, &forced, Utc::now(), &progress)
            .await
            .unwrap();
        assert_eq!(out.removed, 1);
        let cached = store
            .list_worklogs(day("2026-09-01"), day("2026-10-31"), &["alice".into()])
            .unwrap();
        assert_eq!(cached.len(), 1);
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
        let out = fetch_range(&jira, &store, &req, Utc::now(), &|_| {})
            .await
            .unwrap();
        assert!(!out.fetched);
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
