//! Long-running operations shared by commands and the scheduler: session restore, worklog fetch,
//! background refresh.

use crate::state::{AppState, FetchStatus, LastRequest};
use chrono::{NaiveDate, Utc};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use yattat_core::fetch::{fetch_range, FetchProgress, FetchRequest};
use yattat_core::jira::JiraClient;
use yattat_core::models::*;
use yattat_core::secrets;
use yattat_core::settings::WorklogSource;
use yattat_core::tempo::TempoCloud;
use yattat_core::Error;

pub fn emit<T: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!("emit {event} failed: {e}");
    }
}

/// Build a client from a credential, verify it and install it into the state.
pub async fn install_client(
    app: &AppHandle,
    state: &Arc<AppState>,
    credential: Credential,
) -> Result<AuthStatus, String> {
    let settings = state.settings();
    let base = settings.jira_base();
    if base.is_empty() {
        return Err("Keine Jira-URL konfiguriert".into());
    }
    let deployment = state
        .server
        .read()
        .unwrap()
        .as_ref()
        .map(|s| s.deployment)
        .unwrap_or_default();
    let client =
        Arc::new(JiraClient::new(&base, credential, deployment).map_err(|e| e.to_string())?);
    let (user, server) = match client.verify().await {
        Ok(v) => v,
        Err(Error::AuthExpired(code)) => {
            return Err(format!("Anmeldung nicht akzeptiert (HTTP {code})"));
        }
        Err(e) => return Err(e.to_string()),
    };
    // Persist the (possibly rotated) credential and keep it updated.
    if let Some(cookies) = client.cookies() {
        let _ = secrets::save_credential(
            state.secrets.as_ref(),
            &base,
            &Credential::Cookies { cookies },
        );
    }
    {
        let secrets = state.secrets.clone();
        let base_for_listener = base.clone();
        client.set_cookie_listener(Arc::new(move |cookies| {
            let cred = Credential::Cookies {
                cookies: cookies.to_vec(),
            };
            if let Err(e) = secrets::save_credential(secrets.as_ref(), &base_for_listener, &cred) {
                tracing::warn!("could not persist rotated cookies: {e}");
            }
        }));
    }
    state.set_session(client, user, server);
    let status = state.auth_status();
    emit(app, "auth-changed", status.clone());
    Ok(status)
}

/// (Re)builds the Tempo Cloud client from the stored token.
pub fn load_tempo_cloud(state: &Arc<AppState>) {
    let settings = state.settings();
    let token = state
        .secrets
        .get(&secrets::tempo_token_key(&settings.jira_base()))
        .ok()
        .flatten();
    let client = token
        .and_then(|t| TempoCloud::new(&settings.tempo.cloud_api_base, &t).ok())
        .map(Arc::new);
    *state.tempo_cloud.write().unwrap() = client;
}

/// Try to restore a session from the secret store at startup.
pub async fn restore_session(app: &AppHandle, state: &Arc<AppState>) {
    let settings = state.settings();
    let base = settings.jira_base();
    if base.is_empty() {
        return;
    }
    match secrets::load_credential(state.secrets.as_ref(), &base) {
        Ok(Some(credential)) => {
            if let Err(e) = install_client(app, state, credential).await {
                tracing::info!("stored session not usable: {e}");
                state.clear_session(Some(
                    "Gespeicherte Sitzung ist nicht mehr gültig. Bitte erneut anmelden.".into(),
                ));
                emit(app, "auth-changed", state.auth_status());
            }
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!("secret store error: {e}");
            state.clear_session(Some(format!(
                "Zugangsdaten konnten nicht gelesen werden: {e}"
            )));
            emit(app, "auth-changed", state.auth_status());
        }
    }
}

pub fn handle_auth_error(app: &AppHandle, state: &Arc<AppState>, e: &Error) {
    if let Error::AuthExpired(_) = e {
        state.mark_auth_expired();
        emit(app, "auth-changed", state.auth_status());
    }
}

/// Fetch the worklogs of the given members for the range into the cache. Progress is reported
/// through `fetch-status` events; `data-changed` fires when the cache changed.
pub async fn fetch_worklogs(
    app: &AppHandle,
    state: &Arc<AppState>,
    from: NaiveDate,
    to: NaiveDate,
    member_ids: Vec<String>,
    force: bool,
) -> Result<FetchStatus, String> {
    if state.fetch_in_progress.swap(true, Ordering::SeqCst) {
        return Ok(state.fetch_status());
    }
    let settings = state.settings();
    let members = settings.members_by_ids(&member_ids);
    let req = FetchRequest {
        from,
        to,
        author_ids: members.iter().map(|m| m.id.clone()).collect(),
        project_keys: settings.selection.project_keys.clone(),
        force,
    };
    {
        let mut f = state.fetch.write().unwrap();
        f.in_progress = true;
        f.progress = None;
        f.last_error = None;
        f.last_request = Some(LastRequest {
            from,
            to,
            member_ids: req.author_ids.clone(),
        });
        emit(app, "fetch-status", f.clone());
    }
    let tempo = match settings.tempo.source {
        WorklogSource::TempoCloud => match state.tempo_cloud() {
            Some(t) => Some(t),
            None => {
                state.fetch_in_progress.store(false, Ordering::SeqCst);
                let status = {
                    let mut f = state.fetch.write().unwrap();
                    f.in_progress = false;
                    f.last_error = Some(
                        "Tempo Cloud ist als Quelle gewählt, aber es ist kein Tempo-API-Token gespeichert (Einstellungen → Verbindung)."
                            .into(),
                    );
                    f.clone()
                };
                emit(app, "fetch-status", status.clone());
                return Err(status.last_error.clone().unwrap_or_default());
            }
        },
        WorklogSource::JiraWorklogs => None,
    };
    let result = match state.jira() {
        Err(e) => Err(e),
        Ok(jira) => {
            let app2 = app.clone();
            let state2 = state.clone();
            let last_emit = Mutex::new(Instant::now());
            let progress = move |p: FetchProgress| {
                let mut f = state2.fetch.write().unwrap();
                let phase_changed = f
                    .progress
                    .as_ref()
                    .map(|q| q.phase != p.phase)
                    .unwrap_or(true);
                let finished = p.total > 0 && p.done == p.total;
                f.progress = Some(p);
                let mut last = last_emit.lock().unwrap();
                if phase_changed || finished || last.elapsed() >= Duration::from_millis(150) {
                    *last = Instant::now();
                    emit(&app2, "fetch-status", f.clone());
                }
            };
            fetch_range(
                &jira,
                tempo.as_deref(),
                &state.store,
                &req,
                Utc::now(),
                &progress,
            )
            .await
            .map_err(|e| {
                handle_auth_error(app, state, &e);
                e.to_string()
            })
        }
    };
    state.fetch_in_progress.store(false, Ordering::SeqCst);
    let status = {
        let mut f = state.fetch.write().unwrap();
        f.in_progress = false;
        f.progress = None;
        match &result {
            Ok(outcome) => {
                f.last_success = Some(Utc::now());
                f.last_error = None;
                f.last_outcome = Some(outcome.clone());
            }
            Err(e) => f.last_error = Some(e.clone()),
        }
        f.clone()
    };
    emit(app, "fetch-status", status.clone());
    if let Ok(outcome) = &result {
        if outcome.fetched {
            emit(app, "data-changed", ());
        }
    }
    result.map(|_| status)
}

/// Re-fetches the last requested range in the background when auto refresh is on.
pub async fn refresh_loop(app: AppHandle, state: Arc<AppState>) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let settings = state.settings();
        let due = {
            let f = state.fetch.read().unwrap();
            settings.refresh.auto
                && state.is_authenticated()
                && !f.in_progress
                && f.last_request.is_some()
                && f.last_success
                    .map(|t| {
                        (Utc::now() - t).num_seconds()
                            >= settings.refresh.interval_minutes.max(1) as i64 * 60
                    })
                    .unwrap_or(false)
        };
        if !due {
            continue;
        }
        let Some(req) = state.fetch.read().unwrap().last_request.clone() else {
            continue;
        };
        if let Err(e) = fetch_worklogs(&app, &state, req.from, req.to, req.member_ids, true).await {
            tracing::info!("auto refresh failed: {e}");
        }
    }
}
