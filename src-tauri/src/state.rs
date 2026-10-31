//! Shared application state behind `tauri::State<Arc<AppState>>`.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use yattat_core::fetch::{FetchOutcome, FetchProgress};
use yattat_core::jira::JiraClient;
use yattat_core::models::*;
use yattat_core::secrets::{EncryptedSecretStore, SecretStore};
use yattat_core::settings::{AppPaths, Settings};
use yattat_core::storage::Store;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastRequest {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub member_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FetchStatus {
    pub in_progress: bool,
    pub progress: Option<FetchProgress>,
    pub last_success: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_request: Option<LastRequest>,
    pub last_outcome: Option<FetchOutcome>,
}

pub struct AppState {
    pub paths: AppPaths,
    pub store: Arc<Store>,
    pub settings: RwLock<Settings>,
    pub secrets: Arc<dyn SecretStore>,
    pub jira: RwLock<Option<Arc<JiraClient>>>,
    pub user: RwLock<Option<JiraUser>>,
    pub server: RwLock<Option<ServerInfo>>,
    pub auth_message: RwLock<Option<String>>,
    pub fetch: RwLock<FetchStatus>,
    pub login_in_progress: AtomicBool,
    pub fetch_in_progress: AtomicBool,
}

impl AppState {
    pub fn init(paths: AppPaths) -> yattat_core::Result<Self> {
        paths.ensure()?;
        let settings = yattat_core::settings::load_settings(&paths).unwrap_or_else(|e| {
            tracing::warn!("settings could not be loaded, using defaults: {e}");
            Settings::default()
        });
        let store = Arc::new(Store::open(&paths.database_file())?);
        let secrets: Arc<dyn SecretStore> = Arc::new(EncryptedSecretStore::new(&paths));
        Ok(Self {
            paths,
            store,
            settings: RwLock::new(settings),
            secrets,
            jira: RwLock::new(None),
            user: RwLock::new(None),
            server: RwLock::new(None),
            auth_message: RwLock::new(None),
            fetch: RwLock::new(FetchStatus::default()),
            login_in_progress: AtomicBool::new(false),
            fetch_in_progress: AtomicBool::new(false),
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn jira(&self) -> Result<Arc<JiraClient>, String> {
        self.jira
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| "Nicht bei Jira angemeldet".to_string())
    }

    pub fn jira_opt(&self) -> Option<Arc<JiraClient>> {
        self.jira.read().unwrap().clone()
    }

    pub fn user(&self) -> Option<JiraUser> {
        self.user.read().unwrap().clone()
    }

    pub fn is_authenticated(&self) -> bool {
        self.jira.read().unwrap().is_some() && self.user.read().unwrap().is_some()
    }

    pub fn auth_status(&self) -> AuthStatus {
        let settings = self.settings();
        AuthStatus {
            configured: !settings.jira.base_url.is_empty(),
            authenticated: self.is_authenticated(),
            method: self.jira_opt().map(|j| j.credential_kind().to_string()),
            user: self.user(),
            server: self.server.read().unwrap().clone(),
            message: self.auth_message.read().unwrap().clone(),
        }
    }

    pub fn fetch_status(&self) -> FetchStatus {
        self.fetch.read().unwrap().clone()
    }

    /// Install a verified client.
    pub fn set_session(&self, client: Arc<JiraClient>, user: JiraUser, server: ServerInfo) {
        *self.jira.write().unwrap() = Some(client);
        *self.user.write().unwrap() = Some(user);
        *self.server.write().unwrap() = Some(server);
        *self.auth_message.write().unwrap() = None;
    }

    pub fn clear_session(&self, message: Option<String>) {
        *self.jira.write().unwrap() = None;
        *self.user.write().unwrap() = None;
        *self.auth_message.write().unwrap() = message;
    }

    pub fn mark_auth_expired(&self) {
        self.clear_session(Some(
            "Die Jira-Sitzung ist abgelaufen. Bitte erneut anmelden.".into(),
        ));
    }
}
