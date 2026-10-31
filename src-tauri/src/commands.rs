//! Tauri commands: thin wrappers around `yattat-core` and the jobs module.

use crate::jobs::{self, emit};
use crate::state::{AppState, FetchStatus};
use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tauri::{AppHandle, State};
use yattat_core::export::{self, ExportRow};
use yattat_core::models::*;
use yattat_core::secrets;
use yattat_core::settings::{AuthMethod, Settings, PALETTE};
use yattat_core::stats::Stats;
use yattat_core::storage::{CacheInfo, CoverageInfo};
use yattat_core::Error;

type S<'a> = State<'a, Arc<AppState>>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Color of a project: configured, otherwise a stable pick from the palette.
fn project_color(settings: &Settings, key: &str) -> String {
    if let Some(c) = settings.project_colors.get(key) {
        return c.clone();
    }
    let hash = key.bytes().fold(7usize, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(b as usize)
    });
    PALETTE[hash % PALETTE.len()].to_string()
}

// ------------------------------------------------------------------ settings

#[tauri::command]
pub fn get_settings(state: S<'_>) -> Settings {
    state.settings()
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: S<'_>,
    mut settings: Settings,
) -> Result<Settings, String> {
    settings.jira.base_url =
        yattat_core::settings::normalize_jira_url(&settings.jira.base_url).map_err(err)?;
    settings.tidy();
    settings.validate().map_err(err)?;
    let old = state.settings();
    yattat_core::settings::save_settings(&state.paths, &settings).map_err(err)?;
    *state.settings.write().unwrap() = settings.clone();
    if old.jira_base() != settings.jira_base() && state.is_authenticated() {
        state.clear_session(Some("Jira-URL geändert. Bitte erneut anmelden.".into()));
        emit(&app, "auth-changed", state.auth_status());
    }
    emit(&app, "settings-changed", settings.clone());
    Ok(settings)
}

// ---------------------------------------------------------------------- auth

#[tauri::command]
pub fn get_auth_status(state: S<'_>) -> AuthStatus {
    state.auth_status()
}

#[tauri::command]
pub async fn start_browser_login(app: AppHandle, state: S<'_>) -> Result<AuthStatus, String> {
    let status = crate::login::browser_login(app.clone(), state.inner().clone()).await?;
    {
        let mut s = state.settings.write().unwrap();
        s.jira.auth_method = AuthMethod::BrowserSession;
        s.jira.username = status.user.as_ref().map(|u| u.display_name.clone());
        let _ = yattat_core::settings::save_settings(&state.paths, &s);
    }
    Ok(status)
}

#[tauri::command]
pub fn cancel_browser_login(app: AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window(crate::login::LOGIN_WINDOW) {
        let _ = w.close();
    }
}

#[tauri::command]
pub async fn login_with_password(
    app: AppHandle,
    state: S<'_>,
    username: String,
    password: String,
) -> Result<AuthStatus, String> {
    let settings = state.settings();
    let base = settings.jira_base();
    if base.is_empty() {
        return Err("Bitte zuerst die Jira-URL eintragen".into());
    }
    // Validate against the session resource first: gives clear errors (wrong password, CAPTCHA).
    yattat_core::jira::JiraClient::login_with_password(&base, &username, &password)
        .await
        .map_err(err)?;
    let credential = Credential::Basic {
        username: username.clone(),
        password,
    };
    let status = jobs::install_client(&app, &state, credential.clone()).await?;
    secrets::save_credential(state.secrets.as_ref(), &base, &credential).map_err(err)?;
    {
        let mut s = state.settings.write().unwrap();
        s.jira.auth_method = AuthMethod::Password;
        s.jira.username = Some(username);
        let _ = yattat_core::settings::save_settings(&state.paths, &s);
    }
    Ok(status)
}

#[tauri::command]
pub async fn login_with_token(
    app: AppHandle,
    state: S<'_>,
    kind: String,
    user: Option<String>,
    token: String,
) -> Result<AuthStatus, String> {
    let settings = state.settings();
    let base = settings.jira_base();
    if base.is_empty() {
        return Err("Bitte zuerst die Jira-URL eintragen".into());
    }
    let (credential, method) = match kind.as_str() {
        "pat" => (
            Credential::Bearer {
                token: token.trim().to_string(),
            },
            AuthMethod::PersonalAccessToken,
        ),
        "apiToken" => (
            Credential::ApiToken {
                email: user.clone().unwrap_or_default().trim().to_string(),
                token: token.trim().to_string(),
            },
            AuthMethod::ApiToken,
        ),
        other => return Err(format!("Unbekannte Anmeldeart {other}")),
    };
    let status = jobs::install_client(&app, &state, credential.clone()).await?;
    secrets::save_credential(state.secrets.as_ref(), &base, &credential).map_err(err)?;
    {
        let mut s = state.settings.write().unwrap();
        s.jira.auth_method = method;
        s.jira.username = status
            .user
            .as_ref()
            .map(|u| u.display_name.clone())
            .or(user);
        let _ = yattat_core::settings::save_settings(&state.paths, &s);
    }
    Ok(status)
}

#[tauri::command]
pub fn logout(app: AppHandle, state: S<'_>) -> Result<(), String> {
    crate::login::logout(&app, &state)
}

// ------------------------------------------------------------- jira metadata

fn map_jira_error(state: &AppState, e: Error) -> String {
    if matches!(e, Error::AuthExpired(_)) {
        state.mark_auth_expired();
    }
    e.to_string()
}

#[tauri::command]
pub async fn search_projects(state: S<'_>, query: String) -> Result<Vec<Project>, String> {
    let jira = state.jira()?;
    let settings = state.settings();
    let mut projects = jira
        .search_projects(&query)
        .await
        .map_err(|e| map_jira_error(&state, e))?;
    for p in &mut projects {
        p.color = Some(project_color(&settings, &p.key));
    }
    Ok(projects)
}

/// Projects known locally (selected or seen on cached issues), with colors.
#[tauri::command]
pub fn list_projects(state: S<'_>) -> Result<Vec<Project>, String> {
    let settings = state.settings();
    let mut projects = state.store.list_projects().map_err(err)?;
    for p in &mut projects {
        p.color = Some(project_color(&settings, &p.key));
    }
    Ok(projects)
}

#[tauri::command]
pub async fn set_project_selection(
    app: AppHandle,
    state: S<'_>,
    projects: Vec<Project>,
) -> Result<Settings, String> {
    let mut settings = state.settings();
    settings.selection.project_keys = projects.iter().map(|p| p.key.clone()).collect();
    for p in &projects {
        let color = p
            .color
            .clone()
            .unwrap_or_else(|| project_color(&settings, &p.key));
        settings
            .project_colors
            .entry(p.key.clone())
            .or_insert(color);
    }
    state.store.upsert_projects(&projects).map_err(err)?;
    save_settings(app, state, settings).await
}

#[tauri::command]
pub async fn set_project_color(
    app: AppHandle,
    state: S<'_>,
    key: String,
    color: Option<String>,
) -> Result<Settings, String> {
    let mut settings = state.settings();
    match color {
        Some(c) => {
            settings.project_colors.insert(key, c);
        }
        None => {
            settings.project_colors.remove(&key);
        }
    }
    save_settings(app, state, settings).await
}

/// Jira users for the member picker in the team settings.
#[tauri::command]
pub async fn search_users(state: S<'_>, query: String) -> Result<Vec<JiraUser>, String> {
    let jira = state.jira()?;
    jira.search_users(&query)
        .await
        .map_err(|e| map_jira_error(&state, e))
}

// ---------------------------------------------------------------------- data

#[tauri::command]
pub async fn fetch_worklogs(
    app: AppHandle,
    state: S<'_>,
    from: NaiveDate,
    to: NaiveDate,
    member_ids: Vec<String>,
    force: bool,
) -> Result<FetchStatus, String> {
    jobs::fetch_worklogs(&app, &state, from, to, member_ids, force).await
}

#[tauri::command]
pub fn get_fetch_status(state: S<'_>) -> FetchStatus {
    state.fetch_status()
}

#[tauri::command]
pub fn get_coverage(
    state: S<'_>,
    from: NaiveDate,
    to: NaiveDate,
    member_ids: Vec<String>,
) -> Result<CoverageInfo, String> {
    let settings = state.settings();
    let ids: Vec<String> = settings
        .members_by_ids(&member_ids)
        .into_iter()
        .map(|m| m.id)
        .collect();
    state.store.coverage(&ids, from, to).map_err(err)
}

struct Inputs {
    worklogs: Vec<Worklog>,
    issues: HashMap<String, Issue>,
    projects: HashMap<String, Project>,
    members: Vec<Member>,
}

fn project_of(issues: &HashMap<String, Issue>, key: &str) -> String {
    issues
        .get(key)
        .map(|i| i.project_key.clone())
        .unwrap_or_else(|| {
            key.rsplit_once('-')
                .map(|(p, _)| p.to_string())
                .unwrap_or_default()
        })
}

fn inputs(
    state: &AppState,
    from: NaiveDate,
    to: NaiveDate,
    member_ids: &[String],
) -> Result<Inputs, String> {
    let settings = state.settings();
    let members = settings.members_by_ids(member_ids);
    let ids: Vec<String> = members.iter().map(|m| m.id.clone()).collect();
    let issues = state.store.issue_map().map_err(err)?;
    let mut worklogs = state.store.list_worklogs(from, to, &ids).map_err(err)?;
    if !settings.selection.project_keys.is_empty() {
        let allowed: HashSet<&str> = settings
            .selection
            .project_keys
            .iter()
            .map(String::as_str)
            .collect();
        worklogs.retain(|w| allowed.contains(project_of(&issues, &w.issue_key).as_str()));
    }
    let mut projects: HashMap<String, Project> = state
        .store
        .list_projects()
        .map_err(err)?
        .into_iter()
        .map(|mut p| {
            p.color = Some(project_color(&settings, &p.key));
            (p.key.clone(), p)
        })
        .collect();
    for i in issues.values() {
        projects
            .entry(i.project_key.clone())
            .or_insert_with(|| Project {
                id: String::new(),
                key: i.project_key.clone(),
                name: i
                    .project_name
                    .clone()
                    .unwrap_or_else(|| i.project_key.clone()),
                avatar_url: None,
                color: Some(project_color(&settings, &i.project_key)),
            });
    }
    Ok(Inputs {
        worklogs,
        issues,
        projects,
        members,
    })
}

#[tauri::command]
pub async fn get_stats(
    state: S<'_>,
    from: NaiveDate,
    to: NaiveDate,
    member_ids: Vec<String>,
) -> Result<Stats, String> {
    let settings = state.settings();
    let input = inputs(&state, from, to, &member_ids)?;
    Ok(yattat_core::stats::compute(
        &input.worklogs,
        &input.issues,
        &input.projects,
        &input.members,
        from,
        to,
        &settings.work,
        Local::now().date_naive(),
    ))
}

fn rows(
    state: &AppState,
    from: NaiveDate,
    to: NaiveDate,
    member_ids: &[String],
    issue_key: Option<&str>,
    summarize: bool,
) -> Result<Vec<ExportRow>, String> {
    let input = inputs(state, from, to, member_ids)?;
    let worklogs: Vec<Worklog> = match issue_key {
        Some(k) => input
            .worklogs
            .into_iter()
            .filter(|w| w.issue_key == k)
            .collect(),
        None => input.worklogs,
    };
    let rows = export::build_rows(&worklogs, &input.issues, &input.members);
    Ok(if summarize {
        export::summarize_rows(&rows)
    } else {
        rows
    })
}

/// Worklog rows for the detail tables (optionally of a single ticket).
#[tauri::command]
pub async fn list_worklogs(
    state: S<'_>,
    from: NaiveDate,
    to: NaiveDate,
    member_ids: Vec<String>,
    issue_key: Option<String>,
    summarize: bool,
) -> Result<Vec<ExportRow>, String> {
    rows(
        &state,
        from,
        to,
        &member_ids,
        issue_key.as_deref(),
        summarize,
    )
}

#[tauri::command]
pub async fn export_worklogs(
    state: S<'_>,
    from: NaiveDate,
    to: NaiveDate,
    member_ids: Vec<String>,
    format: String,
    summarize: bool,
    path: String,
) -> Result<usize, String> {
    let rows = rows(&state, from, to, &member_ids, None, summarize)?;
    let settings = state.settings();
    let content = match format.as_str() {
        "csv" => export::to_csv(&rows, settings.work.time_format).map_err(err)?,
        "json" => export::to_json(&rows).map_err(err)?,
        other => return Err(format!("Unbekanntes Format {other}")),
    };
    std::fs::write(&path, content).map_err(err)?;
    Ok(rows.len())
}

#[tauri::command]
pub fn get_issue_url(state: S<'_>, key: String) -> String {
    format!("{}/browse/{}", state.settings().jira_base(), key)
}

#[tauri::command]
pub fn clear_cache(app: AppHandle, state: S<'_>) -> Result<CacheInfo, String> {
    state.store.clear().map_err(err)?;
    {
        let mut f = state.fetch.write().unwrap();
        f.last_success = None;
        f.last_outcome = None;
        f.last_request = None;
    }
    emit(&app, "fetch-status", state.fetch_status());
    emit(&app, "data-changed", ());
    state.store.cache_info().map_err(err)
}

// ----------------------------------------------------------------------- info

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub config_dir: String,
    pub data_dir: String,
    pub log_dir: String,
    pub secret_backend: yattat_core::secrets::SecretBackend,
    pub cache: CacheInfo,
}

#[tauri::command]
pub fn get_app_info(state: S<'_>) -> Result<AppInfo, String> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
        config_dir: state.paths.config_dir.display().to_string(),
        data_dir: state.paths.data_dir.display().to_string(),
        log_dir: state.paths.log_dir().display().to_string(),
        secret_backend: state.secrets.backend(),
        cache: state.store.cache_info().map_err(err)?,
    })
}
