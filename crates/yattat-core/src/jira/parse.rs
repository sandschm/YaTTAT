//! Conversion of Jira JSON payloads into the local model. Handles the differences between
//! Jira Cloud and Data Center where they matter (user ids, comment formats).

use crate::models::*;
use chrono::Utc;
use serde_json::Value;

/// Custom field ids discovered via `/rest/api/2/field`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldIds {
    pub epic_link: Option<String>,
}

impl FieldIds {
    pub fn from_fields(fields: &Value) -> FieldIds {
        let mut ids = FieldIds::default();
        if let Some(arr) = fields.as_array() {
            for f in arr {
                let custom = f
                    .pointer("/schema/custom")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let id = f.get("id").and_then(Value::as_str).map(str::to_string);
                if custom == "com.pyxis.greenhopper.jira:gh-epic-link" {
                    ids.epic_link = id;
                }
            }
        }
        ids
    }

    /// Field list for issue search requests.
    pub fn search_fields(&self) -> String {
        let mut fields = vec![
            "summary",
            "issuetype",
            "status",
            "parent",
            "assignee",
            "timetracking",
            "updated",
            "project",
            "labels",
            "components",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        if let Some(e) = &self.epic_link {
            fields.push(e.clone());
        }
        fields.join(",")
    }
}

fn str_at<'a>(v: &'a Value, pointer: &str) -> Option<&'a str> {
    v.pointer(pointer).and_then(Value::as_str)
}

pub fn parse_server_info(v: &Value) -> ServerInfo {
    let deployment = match str_at(v, "/deploymentType") {
        Some("Cloud") => Deployment::Cloud,
        Some(_) => Deployment::DataCenter,
        None => Deployment::Unknown,
    };
    ServerInfo {
        base_url: str_at(v, "/baseUrl").unwrap_or("").to_string(),
        version: str_at(v, "/version").unwrap_or("").to_string(),
        deployment,
        server_title: str_at(v, "/serverTitle").map(str::to_string),
    }
}

pub fn parse_user(v: &Value) -> Option<JiraUser> {
    let id = str_at(v, "/accountId")
        .or_else(|| str_at(v, "/name"))
        .or_else(|| str_at(v, "/key"))?
        .to_string();
    Some(JiraUser {
        id,
        key: str_at(v, "/key").map(str::to_string),
        display_name: str_at(v, "/displayName").unwrap_or("").to_string(),
        email: str_at(v, "/emailAddress").map(str::to_string),
        avatar_url: str_at(v, "/avatarUrls/48x48").map(str::to_string),
        time_zone: str_at(v, "/timeZone").map(str::to_string),
        active: v.get("active").and_then(Value::as_bool).unwrap_or(true),
    })
}

pub fn parse_project(v: &Value) -> Option<Project> {
    Some(Project {
        id: str_at(v, "/id")?.to_string(),
        key: str_at(v, "/key")?.to_string(),
        name: str_at(v, "/name").unwrap_or("").to_string(),
        avatar_url: str_at(v, "/avatarUrls/48x48").map(str::to_string),
        color: None,
    })
}

fn id_string(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::to_string)
        .or_else(|| v.as_i64().map(|i| i.to_string()))
}

pub fn parse_issue(v: &Value, ids: &FieldIds) -> Option<Issue> {
    let key = str_at(v, "/key")?.to_string();
    let id = v.get("id").and_then(id_string)?;
    let fields = v.get("fields")?;
    let project_key = str_at(fields, "/project/key")
        .map(str::to_string)
        .unwrap_or_else(|| {
            key.rsplit_once('-')
                .map(|(p, _)| p.to_string())
                .unwrap_or_default()
        });
    let parent_key = str_at(fields, "/parent/key").map(str::to_string);
    let parent_is_epic = str_at(fields, "/parent/fields/issuetype/name")
        .map(|n| n.eq_ignore_ascii_case("epic"))
        .unwrap_or(false);
    let epic_key = ids
        .epic_link
        .as_ref()
        .and_then(|f| fields.get(f))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            if parent_is_epic {
                parent_key.clone()
            } else {
                None
            }
        });
    let is_subtask = fields
        .pointer("/issuetype/subtask")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let list = |name: &str, inner: Option<&str>| -> Vec<String> {
        fields
            .get(name)
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| match inner {
                        Some(k) => c.get(k).and_then(Value::as_str),
                        None => c.as_str(),
                    })
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    Some(Issue {
        id,
        key,
        project_key,
        project_name: str_at(fields, "/project/name").map(str::to_string),
        summary: str_at(fields, "/summary").unwrap_or("").to_string(),
        issue_type: str_at(fields, "/issuetype/name").unwrap_or("").to_string(),
        issue_type_icon: str_at(fields, "/issuetype/iconUrl").map(str::to_string),
        is_subtask,
        status: str_at(fields, "/status/name").unwrap_or("").to_string(),
        status_category: str_at(fields, "/status/statusCategory/key")
            .unwrap_or("undefined")
            .to_string(),
        parent_key: if is_subtask || !parent_is_epic {
            parent_key
        } else {
            None
        },
        epic_key,
        assignee: str_at(fields, "/assignee/displayName").map(str::to_string),
        assignee_id: str_at(fields, "/assignee/accountId")
            .or_else(|| str_at(fields, "/assignee/name"))
            .or_else(|| str_at(fields, "/assignee/key"))
            .map(str::to_string),
        original_estimate_s: fields
            .pointer("/timetracking/originalEstimateSeconds")
            .and_then(Value::as_i64),
        time_spent_s: fields
            .pointer("/timetracking/timeSpentSeconds")
            .and_then(Value::as_i64),
        labels: list("labels", None),
        components: list("components", Some("name")),
        updated: str_at(fields, "/updated").map(str::to_string),
    })
}

pub fn parse_worklog(v: &Value, issue_key: &str) -> Option<Worklog> {
    let id = v.get("id").and_then(id_string)?;
    let comment = match v.get("comment") {
        Some(Value::String(s)) => s.clone(),
        Some(c @ Value::Object(_)) => adf_to_text(c),
        _ => String::new(),
    };
    let started = super::parse_jira_timestamp(str_at(v, "/started")?)?;
    Some(Worklog {
        id,
        issue_key: issue_key.to_string(),
        author_id: str_at(v, "/author/accountId")
            .or_else(|| str_at(v, "/author/name"))
            .or_else(|| str_at(v, "/author/key"))?
            .to_string(),
        author_name: str_at(v, "/author/displayName").map(str::to_string),
        date: started.date_naive(),
        started,
        seconds: v
            .get("timeSpentSeconds")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        comment,
        updated: str_at(v, "/updated")
            .and_then(super::parse_jira_timestamp)
            .map(|d| d.with_timezone(&Utc)),
    })
}

/// Flatten an Atlassian Document Format node to plain text.
pub fn adf_to_text(v: &Value) -> String {
    let mut out = String::new();
    fn walk(v: &Value, out: &mut String) {
        if let Some(t) = v.get("text").and_then(Value::as_str) {
            out.push_str(t);
        }
        if let Some(children) = v.get("content").and_then(Value::as_array) {
            for (i, c) in children.iter().enumerate() {
                if i > 0 && c.get("type").and_then(Value::as_str) == Some("paragraph") {
                    out.push('\n');
                }
                walk(c, out);
            }
        }
    }
    walk(v, &mut out);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_issue_with_estimates_and_parent() {
        let ids = FieldIds {
            epic_link: Some("customfield_10014".into()),
        };
        let v = json!({
            "id": "10001",
            "key": "ABC-7",
            "fields": {
                "summary": "Do things",
                "project": {"key": "ABC", "name": "Alphabet"},
                "issuetype": {"id": "3", "name": "Sub-task", "subtask": true, "iconUrl": "https://x/i.png"},
                "status": {"name": "In Progress", "statusCategory": {"key": "indeterminate"}},
                "parent": {"key": "ABC-1", "fields": {"issuetype": {"name": "Story"}}},
                "assignee": {"accountId": "acc-1", "displayName": "Sam"},
                "timetracking": {"originalEstimateSeconds": 7200, "timeSpentSeconds": 600},
                "customfield_10014": "ABC-100",
                "labels": ["team-blau"],
                "components": [{"name": "Auth"}],
                "updated": "2026-10-30T10:00:00.000+0100"
            }
        });
        let issue = parse_issue(&v, &ids).unwrap();
        assert_eq!(issue.project_key, "ABC");
        assert_eq!(issue.project_name.as_deref(), Some("Alphabet"));
        assert!(issue.is_subtask);
        assert_eq!(issue.parent_key.as_deref(), Some("ABC-1"));
        assert_eq!(issue.epic_key.as_deref(), Some("ABC-100"));
        assert_eq!(issue.original_estimate_s, Some(7200));
        assert_eq!(issue.time_spent_s, Some(600));
        assert_eq!(issue.assignee_id.as_deref(), Some("acc-1"));
        assert_eq!(issue.labels, vec!["team-blau".to_string()]);
        assert_eq!(issue.components, vec!["Auth".to_string()]);
    }

    #[test]
    fn epic_parent_becomes_epic_key() {
        let ids = FieldIds::default();
        let v = json!({
            "id": 5, "key": "ABC-9",
            "fields": {
                "summary": "Story", "issuetype": {"name": "Story", "subtask": false},
                "status": {"name": "To Do", "statusCategory": {"key": "new"}},
                "parent": {"key": "ABC-2", "fields": {"issuetype": {"name": "Epic"}}}
            }
        });
        let issue = parse_issue(&v, &ids).unwrap();
        assert_eq!(issue.epic_key.as_deref(), Some("ABC-2"));
        assert_eq!(issue.parent_key, None);
        assert_eq!(issue.id, "5");
    }

    #[test]
    fn field_discovery() {
        let v = json!([
            {"id": "customfield_10020", "name": "Sprint", "schema": {"custom": "com.pyxis.greenhopper.jira:gh-sprint"}},
            {"id": "customfield_10014", "name": "Epic Link", "schema": {"custom": "com.pyxis.greenhopper.jira:gh-epic-link"}},
            {"id": "summary", "name": "Summary"}
        ]);
        let ids = FieldIds::from_fields(&v);
        assert_eq!(ids.epic_link.as_deref(), Some("customfield_10014"));
        assert!(ids.search_fields().contains("customfield_10014"));
        assert!(ids.search_fields().contains("timetracking"));
    }

    #[test]
    fn worklog_cloud_and_dc() {
        let cloud = json!({
            "id": 42,
            "author": {"accountId": "acc", "displayName": "Sam"},
            "started": "2026-10-30T23:30:00.000+0100",
            "timeSpentSeconds": 5400,
            "updated": "2026-10-31T08:00:00.000+0100",
            "comment": {"type": "doc", "content": [
                {"type": "paragraph", "content": [{"type": "text", "text": "Hello "}, {"type": "text", "text": "world"}]},
                {"type": "paragraph", "content": [{"type": "text", "text": "Second"}]}
            ]}
        });
        let w = parse_worklog(&cloud, "A-1").unwrap();
        assert_eq!(w.id, "42");
        assert_eq!(w.author_id, "acc");
        assert_eq!(w.comment, "Hello world\nSecond");
        // 23:30 +01:00 is still the 30th for the author, although it is the 31st in UTC+2.
        assert_eq!(w.date.to_string(), "2026-10-30");
        assert_eq!(w.seconds, 5400);
        assert!(w.updated.is_some());

        let dc = json!({
            "id": "7",
            "author": {"name": "sam", "key": "JIRAUSER1", "displayName": "Sam"},
            "started": "2026-10-30T09:00:00.000+0100",
            "timeSpentSeconds": 600,
            "comment": "plain"
        });
        let w = parse_worklog(&dc, "A-1").unwrap();
        assert_eq!(w.author_id, "sam");
        assert_eq!(w.comment, "plain");
        assert_eq!(w.updated, None);
        assert!(parse_worklog(&json!({"id": "1"}), "A-1").is_none());
    }

    #[test]
    fn user_ids_cloud_and_dc() {
        let cloud = json!({"accountId": "abc", "displayName": "A", "emailAddress": "a@x.de", "active": false});
        let dc = json!({"name": "auser", "key": "JIRAUSER1", "displayName": "A"});
        let c = parse_user(&cloud).unwrap();
        assert_eq!(c.id, "abc");
        assert!(!c.active);
        let dc_user = parse_user(&dc).unwrap();
        assert_eq!(dc_user.id, "auser");
        assert_eq!(dc_user.key.as_deref(), Some("JIRAUSER1"));
        assert!(dc_user.active);
    }
}
