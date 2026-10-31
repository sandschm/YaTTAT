//! Jira REST client. Works against Jira Cloud and Jira Data Center with any of the supported
//! credentials. YaTTAT only reads: users, projects, issues and their worklogs.
//! Authentication state is never persisted here; see [`crate::secrets`].

pub mod parse;

use crate::error::{Error, Result};
use crate::models::*;
use chrono::{DateTime, FixedOffset};
use parse::FieldIds;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, COOKIE};
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde_json::{json, Value};
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub const USER_AGENT: &str = concat!("YaTTAT/", env!("CARGO_PKG_VERSION"));

/// Called whenever the server rotated session cookies, so the caller can persist them.
pub type CookieListener = Arc<dyn Fn(&[StoredCookie]) + Send + Sync>;

pub struct JiraClient {
    http: reqwest::Client,
    base: String,
    credential: RwLock<Credential>,
    deployment: RwLock<Deployment>,
    field_ids: RwLock<Option<FieldIds>>,
    cookie_listener: RwLock<Option<CookieListener>>,
}

/// Parse Jira's timestamp format (`2026-10-30T09:00:00.000+0100`, also accepts RFC 3339),
/// keeping the UTC offset Jira sent.
pub fn parse_jira_timestamp(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.3f%z")
        .or_else(|_| DateTime::parse_from_rfc3339(s))
        .ok()
}

/// Quote a value for JQL.
pub fn jql_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn cookie_header(cookies: &[StoredCookie]) -> String {
    cookies
        .iter()
        .map(|c| format!("{}={}", c.name, c.value))
        .collect::<Vec<_>>()
        .join("; ")
}

impl JiraClient {
    pub fn new(base_url: &str, credential: Credential, deployment: Deployment) -> Result<Self> {
        let base = base_url.trim_end_matches('/').to_string();
        let parsed = url::Url::parse(&base)?;
        if parsed.host_str().is_none() {
            return Err(Error::InvalidUrl(base));
        }
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        // Non-form requests are exempt from XSRF checks when this header is present.
        headers.insert("X-Atlassian-Token", HeaderValue::from_static("no-check"));
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            http,
            base,
            credential: RwLock::new(credential),
            deployment: RwLock::new(deployment),
            field_ids: RwLock::new(None),
            cookie_listener: RwLock::new(None),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn deployment(&self) -> Deployment {
        *self.deployment.read().unwrap()
    }

    pub fn credential_kind(&self) -> &'static str {
        self.credential.read().unwrap().kind()
    }

    /// Current cookies, if the client uses a cookie session.
    pub fn cookies(&self) -> Option<Vec<StoredCookie>> {
        match &*self.credential.read().unwrap() {
            Credential::Cookies { cookies } => Some(cookies.clone()),
            _ => None,
        }
    }

    pub fn set_cookie_listener(&self, listener: CookieListener) {
        *self.cookie_listener.write().unwrap() = Some(listener);
    }

    pub fn issue_url(&self, key: &str) -> String {
        format!("{}/browse/{}", self.base, key)
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}/{}", self.base, path.trim_start_matches('/'));
        let rb = self.http.request(method, url);
        match &*self.credential.read().unwrap() {
            Credential::Cookies { cookies } => rb.header(COOKIE, cookie_header(cookies)),
            Credential::Basic { username, password } => rb.basic_auth(username, Some(password)),
            Credential::Bearer { token } => rb.bearer_auth(token),
            Credential::ApiToken { email, token } => rb.basic_auth(email, Some(token)),
        }
    }

    /// Sends the request, retries on HTTP 429 (honouring `Retry-After`) and absorbs rotated
    /// session cookies from `Set-Cookie` headers.
    async fn execute(&self, rb: RequestBuilder) -> Result<Response> {
        let mut attempt = 0;
        let mut rb = rb;
        loop {
            let retry = rb.try_clone();
            let resp = rb.send().await?;
            if resp.status() == StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
                if let Some(rb2) = retry {
                    let wait = resp
                        .headers()
                        .get("Retry-After")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(5)
                        .min(30);
                    tracing::warn!("Jira rate limit, retrying in {wait}s");
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                    attempt += 1;
                    rb = rb2;
                    continue;
                }
            }
            self.absorb_set_cookies(&resp);
            return self.check(resp).await;
        }
    }

    fn absorb_set_cookies(&self, resp: &Response) {
        let headers = resp.headers().get_all(reqwest::header::SET_COOKIE);
        let mut updates: Vec<(String, String, bool)> = Vec::new();
        for h in headers.iter() {
            if let Ok(raw) = h.to_str() {
                if let Some((name, value)) = raw.split(';').next().and_then(|kv| kv.split_once('='))
                {
                    let lower = raw.to_lowercase();
                    let expired =
                        lower.contains("max-age=0") || lower.contains("expires=thu, 01 jan 1970");
                    updates.push((name.trim().to_string(), value.trim().to_string(), expired));
                }
            }
        }
        if updates.is_empty() {
            return;
        }
        let mut changed = false;
        let mut guard = self.credential.write().unwrap();
        if let Credential::Cookies { cookies } = &mut *guard {
            for (name, value, expired) in updates {
                if expired {
                    let before = cookies.len();
                    cookies.retain(|c| c.name != name);
                    changed |= cookies.len() != before;
                } else if let Some(c) = cookies.iter_mut().find(|c| c.name == name) {
                    if c.value != value {
                        c.value = value;
                        changed = true;
                    }
                } else {
                    cookies.push(StoredCookie {
                        name,
                        value,
                        domain: None,
                        path: Some("/".into()),
                        expires: None,
                        secure: true,
                        http_only: true,
                    });
                    changed = true;
                }
            }
            if changed {
                let snapshot = cookies.clone();
                drop(guard);
                if let Some(listener) = &*self.cookie_listener.read().unwrap() {
                    listener(&snapshot);
                }
            }
        }
    }

    async fn check(&self, resp: Response) -> Result<Response> {
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(Error::AuthExpired(status.as_u16()));
        }
        if status.is_redirection() {
            // Cookie sessions that expired redirect to the login page.
            return Err(Error::AuthExpired(status.as_u16()));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| {
                    let mut msgs: Vec<String> = v
                        .get("errorMessages")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    if let Some(errors) = v.get("errors").and_then(Value::as_object) {
                        for (k, val) in errors {
                            msgs.push(format!("{k}: {}", val.as_str().unwrap_or("")));
                        }
                    }
                    if msgs.is_empty() {
                        None
                    } else {
                        Some(msgs.join("; "))
                    }
                })
                .unwrap_or_else(|| body.chars().take(300).collect());
            return Err(Error::Http {
                status: status.as_u16(),
                message,
            });
        }
        Ok(resp)
    }

    pub(crate) async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let resp = self
            .execute(self.request(Method::GET, path).query(query))
            .await?;
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let text = resp.text().await?;
        if !ct.contains("json") && !text.trim_start().starts_with(['{', '[']) {
            // HTML login page served with 200 -> treat as expired session.
            return Err(Error::AuthExpired(200));
        }
        Ok(serde_json::from_str(&text)?)
    }

    // ------------------------------------------------------------- basics

    pub async fn server_info(&self) -> Result<ServerInfo> {
        let v = self.get_json("rest/api/2/serverInfo", &[]).await?;
        let info = parse::parse_server_info(&v);
        if info.deployment != Deployment::Unknown {
            *self.deployment.write().unwrap() = info.deployment;
        }
        Ok(info)
    }

    pub async fn myself(&self) -> Result<JiraUser> {
        let v = self.get_json("rest/api/2/myself", &[]).await?;
        parse::parse_user(&v).ok_or(Error::IncompleteUser)
    }

    /// Checks the credentials and detects the deployment type.
    pub async fn verify(&self) -> Result<(JiraUser, ServerInfo)> {
        let user = self.myself().await?;
        let info = self.server_info().await?;
        Ok((user, info))
    }

    async fn ensure_deployment(&self) -> Result<Deployment> {
        if self.deployment() == Deployment::Unknown {
            self.server_info().await?;
        }
        Ok(self.deployment())
    }

    pub async fn field_ids(&self) -> Result<FieldIds> {
        if let Some(ids) = self.field_ids.read().unwrap().clone() {
            return Ok(ids);
        }
        let v = self.get_json("rest/api/2/field", &[]).await?;
        let ids = FieldIds::from_fields(&v);
        *self.field_ids.write().unwrap() = Some(ids.clone());
        Ok(ids)
    }

    // -------------------------------------------------------------- users

    /// Users matching `query` (name, display name or e-mail). Cloud needs a non-empty query and
    /// only returns real Atlassian accounts (no apps or customers); Data Center lists everyone
    /// for an empty query. Inactive users are left out.
    pub async fn search_users(&self, query: &str) -> Result<Vec<JiraUser>> {
        let deployment = self.ensure_deployment().await?;
        let q = query.trim();
        let params: Vec<(&str, String)> = if deployment == Deployment::Cloud {
            if q.is_empty() {
                return Ok(vec![]);
            }
            vec![("query", q.to_string()), ("maxResults", "50".to_string())]
        } else {
            vec![
                (
                    "username",
                    if q.is_empty() {
                        ".".to_string()
                    } else {
                        q.to_string()
                    },
                ),
                ("maxResults", "50".to_string()),
                ("includeInactive", "false".to_string()),
            ]
        };
        let v = self.get_json("rest/api/2/user/search", &params).await?;
        let mut users: Vec<JiraUser> = v
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter(|u| {
                u.get("accountType")
                    .and_then(Value::as_str)
                    .map(|t| t == "atlassian")
                    .unwrap_or(true)
            })
            .filter_map(parse::parse_user)
            .filter(|u| u.active)
            .collect();
        users.sort_by_key(|u| u.display_name.to_lowercase());
        users.dedup_by(|a, b| a.id == b.id);
        Ok(users)
    }

    // ----------------------------------------------------------- projects

    /// Projects matching `query` (key or name, case-insensitive). Empty query returns all (max 200).
    pub async fn search_projects(&self, query: &str) -> Result<Vec<Project>> {
        let deployment = self.ensure_deployment().await?;
        let q = query.trim().to_lowercase();
        let mut out = Vec::new();
        if deployment == Deployment::Cloud {
            let mut start = 0;
            loop {
                let mut params = vec![
                    ("maxResults", "50".to_string()),
                    ("startAt", start.to_string()),
                    ("orderBy", "name".to_string()),
                ];
                if !q.is_empty() {
                    params.push(("query", query.trim().to_string()));
                }
                let v = self.get_json("rest/api/2/project/search", &params).await?;
                let values = v
                    .get("values")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let n = values.len();
                out.extend(values.iter().filter_map(parse::parse_project));
                let is_last = v.get("isLast").and_then(Value::as_bool).unwrap_or(true);
                if is_last || n == 0 || out.len() >= 200 {
                    break;
                }
                start += n;
            }
        } else {
            let v = self.get_json("rest/api/2/project", &[]).await?;
            let all = v.as_array().cloned().unwrap_or_default();
            out.extend(all.iter().filter_map(parse::parse_project).filter(|p| {
                q.is_empty()
                    || p.key.to_lowercase().contains(&q)
                    || p.name.to_lowercase().contains(&q)
            }));
        }
        out.sort_by_key(|p| p.name.to_lowercase());
        out.truncate(200);
        Ok(out)
    }

    // -------------------------------------------------------------- issues

    /// Runs a JQL search and returns up to `max_total` issues.
    pub async fn search_issues(&self, jql: &str, max_total: usize) -> Result<Vec<Issue>> {
        let deployment = self.ensure_deployment().await?;
        let ids = self.field_ids().await?;
        let fields = ids.search_fields();
        let mut out = Vec::new();
        if deployment == Deployment::Cloud {
            let mut token: Option<String> = None;
            loop {
                let mut params = vec![
                    ("jql", jql.to_string()),
                    ("fields", fields.clone()),
                    ("maxResults", "100".to_string()),
                ];
                if let Some(t) = &token {
                    params.push(("nextPageToken", t.clone()));
                }
                let v = self.get_json("rest/api/2/search/jql", &params).await?;
                let issues = v
                    .get("issues")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                out.extend(issues.iter().filter_map(|i| parse::parse_issue(i, &ids)));
                token = v
                    .get("nextPageToken")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let is_last = v
                    .get("isLast")
                    .and_then(Value::as_bool)
                    .unwrap_or(token.is_none());
                if is_last || token.is_none() || out.len() >= max_total {
                    break;
                }
            }
        } else {
            let mut start = 0usize;
            loop {
                let params = vec![
                    ("jql", jql.to_string()),
                    ("fields", fields.clone()),
                    ("maxResults", "100".to_string()),
                    ("startAt", start.to_string()),
                ];
                let v = self.get_json("rest/api/2/search", &params).await?;
                let issues = v
                    .get("issues")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let n = issues.len();
                out.extend(issues.iter().filter_map(|i| parse::parse_issue(i, &ids)));
                let total = v.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
                start += n;
                if n == 0 || start >= total || out.len() >= max_total {
                    break;
                }
            }
        }
        out.truncate(max_total);
        Ok(out)
    }

    pub async fn get_issue(&self, key: &str) -> Result<Issue> {
        let ids = self.field_ids().await?;
        let v = self
            .get_json(
                &format!("rest/api/2/issue/{key}"),
                &[("fields", ids.search_fields())],
            )
            .await?;
        parse::parse_issue(&v, &ids).ok_or_else(|| Error::IssueUnreadable(key.to_string()))
    }

    /// Issue by key or numeric id; `None` when it does not exist or is not visible.
    pub async fn get_issue_opt(&self, key_or_id: &str) -> Result<Option<Issue>> {
        match self.get_issue(key_or_id).await {
            Ok(i) => Ok(Some(i)),
            Err(Error::Http { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // ------------------------------------------------------------ worklogs

    /// All worklogs of an issue (every author), following the pagination.
    pub async fn issue_worklogs(&self, issue_key: &str) -> Result<Vec<Worklog>> {
        let mut out = Vec::new();
        let mut start = 0usize;
        loop {
            let v = self
                .get_json(
                    &format!("rest/api/2/issue/{issue_key}/worklog"),
                    &[
                        ("startAt", start.to_string()),
                        ("maxResults", "1000".to_string()),
                    ],
                )
                .await?;
            let logs = v
                .get("worklogs")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let n = logs.len();
            out.extend(
                logs.iter()
                    .filter_map(|w| parse::parse_worklog(w, issue_key)),
            );
            let total = v.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
            start += n;
            if n == 0 || start >= total {
                break;
            }
        }
        Ok(out)
    }

    /// Authenticate against Data Center with username/password and return the session cookie.
    pub async fn login_with_password(
        base_url: &str,
        username: &str,
        password: &str,
    ) -> Result<Vec<StoredCookie>> {
        let base = base_url.trim_end_matches('/');
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(45))
            .build()?;
        let resp = http
            .post(format!("{base}/rest/auth/1/session"))
            .header("X-Atlassian-Token", "no-check")
            .json(&json!({"username": username, "password": password}))
            .send()
            .await?;
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(Error::WrongCredentials);
        }
        if status == StatusCode::FORBIDDEN {
            let reason = resp
                .headers()
                .get("X-Seraph-LoginReason")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if reason.contains("AUTHENTICATION_DENIED") {
                return Err(Error::CaptchaRequired);
            }
            return Err(Error::AuthExpired(403));
        }
        if !status.is_success() {
            return Err(Error::LoginFailed(status.as_u16()));
        }
        let mut cookies = Vec::new();
        for header in resp.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(raw) = header.to_str() {
                if let Some((name, value)) = raw.split(';').next().and_then(|kv| kv.split_once('='))
                {
                    cookies.push(StoredCookie {
                        name: name.trim().to_string(),
                        value: value.trim().to_string(),
                        domain: None,
                        path: Some("/".into()),
                        expires: None,
                        secure: raw.to_lowercase().contains("secure"),
                        http_only: raw.to_lowercase().contains("httponly"),
                    });
                }
            }
        }
        if cookies.is_empty() {
            // Older instances put the session into the JSON body.
            let v: Value = resp.json().await.unwrap_or(Value::Null);
            if let (Some(name), Some(value)) = (
                v.pointer("/session/name").and_then(Value::as_str),
                v.pointer("/session/value").and_then(Value::as_str),
            ) {
                cookies.push(StoredCookie {
                    name: name.into(),
                    value: value.into(),
                    domain: None,
                    path: Some("/".into()),
                    expires: None,
                    secure: true,
                    http_only: true,
                });
            }
        }
        if cookies.is_empty() {
            return Err(Error::NoSessionCookies);
        }
        Ok(cookies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cookie_cred() -> Credential {
        Credential::Cookies {
            cookies: vec![StoredCookie {
                name: "JSESSIONID".into(),
                value: "abc123".into(),
                domain: None,
                path: None,
                expires: None,
                secure: true,
                http_only: true,
            }],
        }
    }

    #[tokio::test]
    async fn sends_cookie_and_xsrf_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/myself"))
            .and(header("Cookie", "JSESSIONID=abc123"))
            .and(header("X-Atlassian-Token", "no-check"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"name": "sam", "displayName": "Sam"})),
            )
            .mount(&server)
            .await;
        let client = JiraClient::new(&server.uri(), cookie_cred(), Deployment::DataCenter).unwrap();
        let me = client.myself().await.unwrap();
        assert_eq!(me.id, "sam");
    }

    #[tokio::test]
    async fn rotated_cookies_are_absorbed_and_reported() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/myself"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Set-Cookie", "JSESSIONID=rotated; Path=/; HttpOnly")
                    .append_header("Set-Cookie", "atlassian.xsrf.token=new; Path=/")
                    .set_body_json(json!({"name": "sam", "displayName": "Sam"})),
            )
            .mount(&server)
            .await;
        let client = JiraClient::new(&server.uri(), cookie_cred(), Deployment::DataCenter).unwrap();
        let seen: Arc<std::sync::Mutex<Vec<StoredCookie>>> = Arc::default();
        let seen2 = seen.clone();
        client.set_cookie_listener(Arc::new(move |c| *seen2.lock().unwrap() = c.to_vec()));
        client.myself().await.unwrap();
        let cookies = client.cookies().unwrap();
        assert_eq!(
            cookies
                .iter()
                .find(|c| c.name == "JSESSIONID")
                .unwrap()
                .value,
            "rotated"
        );
        assert_eq!(cookies.len(), 2);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn rate_limit_is_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/serverInfo"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/serverInfo"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"baseUrl": "x", "version": "9", "deploymentType": "Server"}),
                ),
            )
            .mount(&server)
            .await;
        let client = JiraClient::new(&server.uri(), cookie_cred(), Deployment::Unknown).unwrap();
        let info = client.server_info().await.unwrap();
        assert_eq!(info.deployment, Deployment::DataCenter);
        assert_eq!(client.deployment(), Deployment::DataCenter);
    }

    #[tokio::test]
    async fn expired_session_is_detected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/myself"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", "/login.jsp"))
            .mount(&server)
            .await;
        let client = JiraClient::new(&server.uri(), cookie_cred(), Deployment::DataCenter).unwrap();
        assert!(matches!(
            client.myself().await,
            Err(Error::AuthExpired(302))
        ));
    }

    #[tokio::test]
    async fn user_search_cloud_filters_apps_and_inactive() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/user/search"))
            .and(query_param("query", "sam"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"accountId": "a1", "displayName": "Sam Mueller", "accountType": "atlassian", "active": true, "emailAddress": "sam@x.de", "avatarUrls": {"48x48": "https://a/48"}},
                {"accountId": "bot", "displayName": "Samba Bot", "accountType": "app", "active": true},
                {"accountId": "old", "displayName": "Samuel Alt", "accountType": "atlassian", "active": false},
                {"accountId": "a0", "displayName": "Adam Samson", "accountType": "atlassian", "active": true}
            ])))
            .mount(&server)
            .await;
        let client = JiraClient::new(
            &server.uri(),
            Credential::ApiToken {
                email: "e".into(),
                token: "t".into(),
            },
            Deployment::Cloud,
        )
        .unwrap();
        let users = client.search_users(" sam ").await.unwrap();
        assert_eq!(
            users.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(),
            vec!["a0", "a1"]
        );
        assert_eq!(users[1].avatar_url.as_deref(), Some("https://a/48"));
        assert!(client.search_users("").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn user_search_dc_uses_username_and_dot_wildcard() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/user/search"))
            .and(query_param("username", "."))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"name": "sam", "key": "JIRAUSER1", "displayName": "Sam", "active": true},
                {"name": "amy", "key": "JIRAUSER2", "displayName": "Amy", "active": true}
            ])))
            .mount(&server)
            .await;
        let client = JiraClient::new(&server.uri(), cookie_cred(), Deployment::DataCenter).unwrap();
        let users = client.search_users("").await.unwrap();
        assert_eq!(
            users.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(),
            vec!["amy", "sam"]
        );
        assert_eq!(users[1].key.as_deref(), Some("JIRAUSER1"));
    }

    #[tokio::test]
    async fn dc_search_paginates_with_start_at() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/field"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        let issue = |k: &str| json!({"id": "1", "key": k, "fields": {"summary": k, "project": {"key": "A"}, "issuetype": {"name": "Task"}, "status": {"name": "Open", "statusCategory": {"key": "new"}}}});
        Mock::given(method("GET"))
            .and(path("/rest/api/2/search"))
            .and(query_param("startAt", "0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"issues": [issue("A-1")], "total": 2, "startAt": 0})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/search"))
            .and(query_param("startAt", "1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"issues": [issue("A-2")], "total": 2, "startAt": 1})),
            )
            .mount(&server)
            .await;
        let client = JiraClient::new(
            &server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::DataCenter,
        )
        .unwrap();
        let issues = client.search_issues("project = A", 100).await.unwrap();
        assert_eq!(
            issues.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(),
            vec!["A-1", "A-2"]
        );
    }

    #[tokio::test]
    async fn cloud_search_uses_next_page_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/field"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        let issue = |k: &str| json!({"id": "1", "key": k, "fields": {"summary": k, "issuetype": {"name": "Task"}, "status": {"name": "Open", "statusCategory": {"key": "new"}}}});
        Mock::given(method("GET"))
            .and(path("/rest/api/2/search/jql"))
            .and(query_param("nextPageToken", "p2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"issues": [issue("C-2")], "isLast": true})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/search/jql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"issues": [issue("C-1")], "nextPageToken": "p2", "isLast": false}),
            ))
            .mount(&server)
            .await;
        let client = JiraClient::new(
            &server.uri(),
            Credential::ApiToken {
                email: "e".into(),
                token: "t".into(),
            },
            Deployment::Cloud,
        )
        .unwrap();
        let issues = client.search_issues("project = C", 100).await.unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[1].project_key, "C");
    }

    #[tokio::test]
    async fn worklogs_follow_pagination() {
        let server = MockServer::start().await;
        let log = |id: &str, author: &str| json!({"id": id, "author": {"accountId": author, "displayName": author}, "started": "2026-10-05T09:00:00.000+0200", "timeSpentSeconds": 1800, "comment": "c", "updated": "2026-10-05T10:00:00.000+0200"});
        Mock::given(method("GET"))
            .and(path("/rest/api/2/issue/A-1/worklog"))
            .and(query_param("startAt", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"startAt": 0, "maxResults": 1, "total": 2, "worklogs": [log("1", "a")]}),
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/issue/A-1/worklog"))
            .and(query_param("startAt", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"startAt": 1, "maxResults": 1, "total": 2, "worklogs": [log("2", "b")]}),
            ))
            .mount(&server)
            .await;
        let client = JiraClient::new(
            &server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::Cloud,
        )
        .unwrap();
        let logs = client.issue_worklogs("A-1").await.unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[1].author_id, "b");
        assert_eq!(logs[0].date.to_string(), "2026-10-05");
        assert_eq!(logs[0].seconds, 1800);
    }

    #[tokio::test]
    async fn password_login_collects_cookies() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/auth/1/session"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Set-Cookie", "JSESSIONID=XYZ; Path=/; HttpOnly")
                    .append_header("Set-Cookie", "atlassian.xsrf.token=TOK; Path=/")
                    .set_body_json(json!({"session": {"name": "JSESSIONID", "value": "XYZ"}})),
            )
            .mount(&server)
            .await;
        let cookies = JiraClient::login_with_password(&server.uri(), "u", "p")
            .await
            .unwrap();
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].name, "JSESSIONID");
        assert!(cookies[0].http_only);
    }

    #[test]
    fn timestamp_keeps_offset() {
        let parsed = parse_jira_timestamp("2026-10-30T23:30:00.000+0100").unwrap();
        assert_eq!(parsed.offset().local_minus_utc(), 3600);
        assert_eq!(parsed.date_naive().to_string(), "2026-10-30");
        assert!(parse_jira_timestamp("2026-10-30T09:00:00Z").is_some());
        assert_eq!(jql_quote("a\"b"), "\"a\\\"b\"");
    }
}
