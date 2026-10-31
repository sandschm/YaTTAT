//! Shared data model. Everything here is serialized to the frontend, so field names are camelCase.

use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// How the Jira instance is deployed. Affects endpoints, pagination and user ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Deployment {
    #[default]
    Unknown,
    Cloud,
    DataCenter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub base_url: String,
    pub version: String,
    pub deployment: Deployment,
    pub server_title: Option<String>,
}

fn default_true() -> bool {
    true
}

/// A Jira user as returned by the REST API (the logged-in user or a search result).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JiraUser {
    /// Cloud: accountId, Data Center: username (`name`). This is the value JQL's
    /// `worklogAuthor` expects and the value worklogs carry as author.
    pub id: String,
    /// Data Center user key (e.g. `JIRAUSER10100`).
    pub key: Option<String>,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    /// IANA time zone of the Jira profile.
    pub time_zone: Option<String>,
    #[serde(default = "default_true")]
    pub active: bool,
}

impl JiraUser {
    pub fn to_member(&self) -> Member {
        Member {
            id: self.id.clone(),
            key: self.key.clone(),
            display_name: self.display_name.clone(),
            email: self.email.clone(),
            avatar_url: self.avatar_url.clone(),
            weekly_hours: None,
            color: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub key: String,
    pub name: String,
    pub avatar_url: Option<String>,
    /// Hex color chosen locally (e.g. "#3b82f6").
    pub color: Option<String>,
}

/// An issue as cached locally. Only the fields the analysis needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub key: String,
    pub project_key: String,
    pub project_name: Option<String>,
    pub summary: String,
    pub issue_type: String,
    pub issue_type_icon: Option<String>,
    pub is_subtask: bool,
    pub status: String,
    /// "new" | "indeterminate" | "done" | "undefined"
    pub status_category: String,
    pub parent_key: Option<String>,
    pub epic_key: Option<String>,
    pub assignee: Option<String>,
    pub assignee_id: Option<String>,
    pub original_estimate_s: Option<i64>,
    /// Total time spent on the issue by everyone, as reported by Jira at fetch time.
    pub time_spent_s: Option<i64>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub components: Vec<String>,
    pub updated: Option<String>,
}

/// A worklog as cached locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worklog {
    pub id: String,
    pub issue_key: String,
    /// Same id space as [`JiraUser::id`] / [`Member::id`].
    pub author_id: String,
    pub author_name: Option<String>,
    /// Start as sent by Jira, including the author's UTC offset.
    pub started: DateTime<FixedOffset>,
    /// Calendar day of `started` in the author's time zone. All day-based statistics use this.
    pub date: NaiveDate,
    pub seconds: i64,
    pub comment: String,
    pub updated: Option<DateTime<Utc>>,
}

/// A team member as configured in the settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Member {
    /// Jira user id (Cloud: accountId, Data Center: username).
    pub id: String,
    pub key: Option<String>,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    /// Contract hours per week; scales the weekday pattern of the work settings. `None` = full pattern.
    pub weekly_hours: Option<f32>,
    /// Hex color for charts.
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub members: Vec<Member>,
}

/// A browser cookie captured from the login webview and reused for REST calls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredCookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub expires: Option<i64>,
    pub secure: bool,
    pub http_only: bool,
}

/// Credentials stored encrypted on disk (key in the OS keyring).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Credential {
    /// Session cookies captured from a browser login (SSO, 2FA).
    Cookies { cookies: Vec<StoredCookie> },
    /// Username + password (Data Center). Sent as Basic auth.
    Basic { username: String, password: String },
    /// Personal Access Token (Data Center) – sent as Bearer.
    Bearer { token: String },
    /// E-mail + API token (Cloud) – sent as Basic auth.
    ApiToken { email: String, token: String },
}

impl Credential {
    pub fn kind(&self) -> &'static str {
        match self {
            Credential::Cookies { .. } => "cookies",
            Credential::Basic { .. } => "basic",
            Credential::Bearer { .. } => "bearer",
            Credential::ApiToken { .. } => "apiToken",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub configured: bool,
    pub authenticated: bool,
    pub method: Option<String>,
    pub user: Option<JiraUser>,
    pub server: Option<ServerInfo>,
    pub message: Option<String>,
}
