//! Tempo Cloud REST API v4 as an alternative worklog source.
//!
//! Teams that log their time in Tempo often end up with Jira worklogs whose author is the Tempo
//! app rather than the person (Jira then cannot attribute them to a team member). The Tempo API
//! reports the real author, so YaTTAT can read the worklogs from there with a Tempo API token.

use crate::error::{Error, Result};
use crate::models::Worklog;
use chrono::{DateTime, FixedOffset, Local, LocalResult, NaiveDate, NaiveTime, TimeZone, Utc};
use serde_json::Value;
use std::time::Duration;

pub const DEFAULT_API_BASE: &str = "https://api.tempo.io/4";
const PAGE_SIZE: usize = 500;

pub struct TempoCloud {
    http: reqwest::Client,
    base: String,
    token: String,
}

/// A worklog as returned by Tempo. The issue is only known by its numeric Jira id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempoWorklog {
    pub tempo_id: String,
    pub issue_id: i64,
    pub author_id: String,
    pub start_date: NaiveDate,
    pub start_time: Option<NaiveTime>,
    pub seconds: i64,
    pub billable_seconds: Option<i64>,
    pub description: String,
    pub updated: Option<DateTime<Utc>>,
}

impl TempoWorklog {
    /// Converts to the cache model once the issue key is known. Tempo stores the start as a plain
    /// date and time; it is interpreted in the local time zone of this machine.
    pub fn into_worklog(self, issue_key: &str) -> Worklog {
        let time = self
            .start_time
            .unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let naive = self.start_date.and_time(time);
        let started: DateTime<FixedOffset> = match Local.from_local_datetime(&naive) {
            LocalResult::Single(d) => d.fixed_offset(),
            LocalResult::Ambiguous(a, _) => a.fixed_offset(),
            LocalResult::None => Utc.from_utc_datetime(&naive).fixed_offset(),
        };
        Worklog {
            id: format!("tempo-{}", self.tempo_id),
            issue_key: issue_key.to_string(),
            author_id: self.author_id,
            author_name: None,
            started,
            date: self.start_date,
            seconds: self.seconds,
            comment: self.description,
            updated: self.updated,
        }
    }
}

fn id_string(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::to_string)
        .or_else(|| v.as_i64().map(|i| i.to_string()))
}

pub fn parse_worklog(v: &Value) -> Option<TempoWorklog> {
    let tempo_id = v.get("tempoWorklogId").and_then(id_string)?;
    let issue_id = v.pointer("/issue/id").and_then(|i| {
        i.as_i64()
            .or_else(|| i.as_str().and_then(|s| s.parse().ok()))
    })?;
    let author_id = v
        .pointer("/author/accountId")
        .and_then(Value::as_str)?
        .to_string();
    let start_date = v
        .get("startDate")
        .and_then(Value::as_str)
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())?;
    let start_time = v
        .get("startTime")
        .and_then(Value::as_str)
        .and_then(|s| NaiveTime::parse_from_str(s, "%H:%M:%S").ok());
    Some(TempoWorklog {
        tempo_id,
        issue_id,
        author_id,
        start_date,
        start_time,
        seconds: v
            .get("timeSpentSeconds")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        billable_seconds: v.get("billableSeconds").and_then(Value::as_i64),
        description: v
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        updated: v
            .get("updatedAt")
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc)),
    })
}

impl TempoCloud {
    pub fn new(base: &str, token: &str) -> Result<Self> {
        let base = base.trim().trim_end_matches('/').to_string();
        url::Url::parse(&base)?;
        let http = reqwest::Client::builder()
            .user_agent(crate::jira::USER_AGENT)
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            http,
            base,
            token: token.trim().to_string(),
        })
    }

    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let resp = self
            .http
            .get(format!("{}/{}", self.base, path.trim_start_matches('/')))
            .bearer_auth(&self.token)
            .header("Accept", "application/json")
            .query(query)
            .send()
            .await?;
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            // Deliberately not `AuthExpired`: that would log the user out of Jira.
            return Err(Error::TempoAuth(status.as_u16()));
        }
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| {
                    v.get("errors").and_then(Value::as_array).map(|errs| {
                        errs.iter()
                            .filter_map(|e| e.get("message").and_then(Value::as_str))
                            .collect::<Vec<_>>()
                            .join("; ")
                    })
                })
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| text.chars().take(300).collect());
            return Err(Error::TempoHttp {
                status: status.as_u16(),
                message,
            });
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// Checks the token. With an account id the user's own worklogs are requested, which every
    /// token may read.
    pub async fn verify(&self, account_id: Option<&str>) -> Result<()> {
        let path = match account_id {
            Some(id) => format!("worklogs/user/{id}"),
            None => "worklogs".to_string(),
        };
        self.get(&path, &[("limit", "1".to_string())]).await?;
        Ok(())
    }

    /// Worklogs of one user between `from` and `to` (inclusive), following the pagination.
    pub async fn user_worklogs(
        &self,
        account_id: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<TempoWorklog>> {
        let mut out = Vec::new();
        let mut offset = 0usize;
        loop {
            let v = self
                .get(
                    &format!("worklogs/user/{account_id}"),
                    &[
                        ("from", from.format("%Y-%m-%d").to_string()),
                        ("to", to.format("%Y-%m-%d").to_string()),
                        ("offset", offset.to_string()),
                        ("limit", PAGE_SIZE.to_string()),
                    ],
                )
                .await?;
            let results = v
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let n = results.len();
            out.extend(results.iter().filter_map(parse_worklog));
            let has_next = v
                .pointer("/metadata/next")
                .and_then(Value::as_str)
                .is_some();
            offset += n;
            if n == 0 || n < PAGE_SIZE || !has_next {
                break;
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn parses_tempo_worklog_and_converts() {
        let v = json!({
            "tempoWorklogId": 126,
            "issue": {"self": "https://x.atlassian.net/rest/api/2/issue/10001", "id": 10001},
            "timeSpentSeconds": 3600,
            "billableSeconds": 1800,
            "startDate": "2026-09-01",
            "startTime": "09:30:00",
            "description": "Pairing",
            "updatedAt": "2026-09-30T22:31:57Z",
            "author": {"accountId": "712020:abc"}
        });
        let w = parse_worklog(&v).unwrap();
        assert_eq!(w.tempo_id, "126");
        assert_eq!(w.issue_id, 10001);
        assert_eq!(w.billable_seconds, Some(1800));
        let converted = w.into_worklog("A-1");
        assert_eq!(converted.id, "tempo-126");
        assert_eq!(converted.date.to_string(), "2026-09-01");
        assert_eq!(converted.started.format("%H:%M").to_string(), "09:30");
        assert_eq!(converted.comment, "Pairing");
        assert!(converted.updated.is_some());
        // without a start time the worklog starts at midnight
        let v2 = json!({"tempoWorklogId": "7", "issue": {"id": "42"}, "startDate": "2026-09-02", "author": {"accountId": "a"}});
        let w2 = parse_worklog(&v2).unwrap().into_worklog("B-2");
        assert_eq!(w2.started.format("%H:%M").to_string(), "00:00");
        assert_eq!(w2.seconds, 0);
        assert!(parse_worklog(&json!({"tempoWorklogId": 1})).is_none());
    }

    #[tokio::test]
    async fn user_worklogs_follow_pagination_and_send_bearer() {
        let server = MockServer::start().await;
        let log = |id: i64| json!({"tempoWorklogId": id, "issue": {"id": 10001}, "timeSpentSeconds": 60, "startDate": "2026-09-01", "author": {"accountId": "a"}});
        let page1: Vec<Value> = (0..PAGE_SIZE as i64).map(log).collect();
        Mock::given(method("GET"))
            .and(path("/worklogs/user/a"))
            .and(header("Authorization", "Bearer tok"))
            .and(query_param("offset", "0"))
            .and(query_param("from", "2026-09-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "metadata": {"count": PAGE_SIZE, "offset": 0, "limit": PAGE_SIZE, "next": "https://x/next"},
                "results": page1
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/worklogs/user/a"))
            .and(query_param("offset", PAGE_SIZE.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "metadata": {"count": 1, "offset": PAGE_SIZE, "limit": PAGE_SIZE},
                "results": [log(9999)]
            })))
            .mount(&server)
            .await;
        let tempo = TempoCloud::new(&server.uri(), "tok").unwrap();
        let logs = tempo
            .user_worklogs("a", day("2026-09-01"), day("2026-09-30"))
            .await
            .unwrap();
        assert_eq!(logs.len(), PAGE_SIZE + 1);
        assert_eq!(logs.last().unwrap().tempo_id, "9999");
    }

    #[tokio::test]
    async fn auth_errors_do_not_look_like_an_expired_jira_session() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/worklogs/user/me"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/worklogs"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({"errors": [{"message": "limit too large"}]})),
            )
            .mount(&server)
            .await;
        let tempo = TempoCloud::new(&server.uri(), "tok").unwrap();
        match tempo.verify(Some("me")).await {
            Err(Error::TempoAuth(403)) => {}
            other => panic!("unexpected {other:?}"),
        }
        match tempo.verify(None).await {
            Err(Error::TempoHttp {
                status: 400,
                message,
            }) => assert!(message.contains("limit too large")),
            other => panic!("unexpected {other:?}"),
        }
    }
}
