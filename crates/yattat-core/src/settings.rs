//! Application settings. Stored as JSON in the user's config directory.
//! Secrets never live here, they go to [`crate::secrets`].

use crate::error::{Error, Result};
use crate::models::{Member, Team};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "YaTTAT";

/// Colors handed out to members (and projects) that have none yet.
pub const PALETTE: [&str; 12] = [
    "#2f6fed", "#17956b", "#e8572a", "#8b5cf6", "#ec4899", "#0ea5e9", "#f59e0b", "#14b8a6",
    "#ef4444", "#84cc16", "#6366f1", "#a16207",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    /// Login in an embedded browser window; session cookies are reused (SSO, 2FA).
    #[default]
    BrowserSession,
    /// Username and password (Jira Data Center).
    Password,
    /// Personal Access Token (Jira Data Center).
    PersonalAccessToken,
    /// E-mail and API token (Jira Cloud).
    ApiToken,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JiraSettings {
    pub base_url: String,
    pub auth_method: AuthMethod,
    /// Display only, the actual secret is in the keyring.
    pub username: Option<String>,
}

impl Default for JiraSettings {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            auth_method: AuthMethod::BrowserSession,
            username: None,
        }
    }
}

/// Which projects the analysis covers. Empty = all projects the members logged time on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SelectionSettings {
    pub project_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RefreshSettings {
    /// Re-fetch the currently displayed range in the background.
    pub auto: bool,
    pub interval_minutes: u32,
}

impl Default for RefreshSettings {
    fn default() -> Self {
        Self {
            auto: true,
            interval_minutes: 30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TimeFormat {
    #[default]
    HoursMinutes,
    Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkSettings {
    /// Target hours Monday..Sunday for a full-time member.
    pub hours_per_weekday: [f32; 7],
    /// 0 = Monday … 6 = Sunday
    pub first_day_of_week: u8,
    pub time_format: TimeFormat,
    /// Show target hours and balances in the statistics.
    pub show_targets: bool,
}

impl Default for WorkSettings {
    fn default() -> Self {
        Self {
            hours_per_weekday: [8.0, 8.0, 8.0, 8.0, 8.0, 0.0, 0.0],
            first_day_of_week: 0,
            time_format: TimeFormat::HoursMinutes,
            show_targets: true,
        }
    }
}

impl WorkSettings {
    /// Hours of the full-time pattern per week.
    pub fn pattern_hours_per_week(&self) -> f32 {
        self.hours_per_weekday.iter().sum()
    }
}

/// Where worklogs are read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WorklogSource {
    /// Jira's worklog API. Works when people log time directly in Jira (or Tempo writes the Jira
    /// worklogs under their own account).
    #[default]
    JiraWorklogs,
    /// Tempo Cloud API with a Tempo API token. Needed when Jira worklogs carry the Tempo app as
    /// author, which hides the real person from the Jira API.
    TempoCloud,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TempoSettings {
    pub source: WorklogSource,
    pub cloud_api_base: String,
    /// Set when a Tempo Cloud token is present in the secret store.
    pub cloud_token_set: bool,
}

impl Default for TempoSettings {
    fn default() -> Self {
        Self {
            source: WorklogSource::JiraWorklogs,
            cloud_api_base: crate::tempo::DEFAULT_API_BASE.to_string(),
            cloud_token_set: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BehaviorSettings {
    pub theme: Theme,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub jira: JiraSettings,
    pub selection: SelectionSettings,
    /// Teams and their members. A member may belong to several teams.
    pub teams: Vec<Team>,
    pub work: WorkSettings,
    pub refresh: RefreshSettings,
    pub behavior: BehaviorSettings,
    pub tempo: TempoSettings,
    /// Project key -> hex color.
    pub project_colors: BTreeMap<String, String>,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !self.jira.base_url.trim().is_empty() {
            normalize_jira_url(&self.jira.base_url)?;
        }
        if self.refresh.interval_minutes == 0 && self.refresh.auto {
            return Err(Error::Invalid(
                "Aktualisierungsintervall muss mindestens 1 Minute sein".into(),
            ));
        }
        if self.tempo.source == WorklogSource::TempoCloud {
            url::Url::parse(self.tempo.cloud_api_base.trim())
                .map_err(|_| Error::Invalid("Tempo-API-Basis ist keine gültige URL".into()))?;
        }
        for t in &self.teams {
            for m in &t.members {
                if let Some(h) = m.weekly_hours {
                    if !(0.0..=80.0).contains(&h) {
                        return Err(Error::Invalid(format!(
                            "Wochenstunden von {} müssen zwischen 0 und 80 liegen",
                            m.display_name
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Normalized base URL without trailing slash (see [`normalize_jira_url`]).
    pub fn jira_base(&self) -> String {
        normalize_jira_url(&self.jira.base_url)
            .unwrap_or_else(|_| self.jira.base_url.trim_end_matches('/').to_string())
    }

    /// Cleans up teams: trims names, drops members without id, removes duplicate members per team,
    /// hands out colors to members without one and clamps the weekday pattern.
    pub fn tidy(&mut self) {
        for h in self.hours_per_weekday_mut() {
            if !h.is_finite() {
                *h = 0.0;
            }
            *h = h.clamp(0.0, 24.0);
        }
        let mut used: HashSet<String> = HashSet::new();
        for t in &self.teams {
            for m in &t.members {
                if let Some(c) = &m.color {
                    used.insert(c.to_lowercase());
                }
            }
        }
        // The same member keeps the same color in every team.
        let mut assigned: BTreeMap<String, String> = BTreeMap::new();
        let mut next = 0usize;
        for (i, t) in self.teams.iter_mut().enumerate() {
            t.name = t.name.trim().to_string();
            if t.name.is_empty() {
                t.name = format!("Team {}", i + 1);
            }
            if t.id.trim().is_empty() {
                t.id = format!("team-{}", i + 1);
            }
            let mut seen: HashSet<String> = HashSet::new();
            t.members
                .retain(|m| !m.id.trim().is_empty() && seen.insert(m.id.clone()));
            for m in &mut t.members {
                m.display_name = m.display_name.trim().to_string();
                if m.display_name.is_empty() {
                    m.display_name = m.id.clone();
                }
                if m.color.as_deref().map(str::trim).unwrap_or("").is_empty() {
                    let color = assigned.get(&m.id).cloned().unwrap_or_else(|| {
                        // first palette color nobody uses yet, else round-robin
                        let fresh = PALETTE
                            .iter()
                            .find(|c| !used.contains(&c.to_lowercase()))
                            .map(|c| c.to_string());
                        let c = fresh.unwrap_or_else(|| {
                            let c = PALETTE[next % PALETTE.len()].to_string();
                            next += 1;
                            c
                        });
                        used.insert(c.to_lowercase());
                        c
                    });
                    assigned.insert(m.id.clone(), color.clone());
                    m.color = Some(color);
                } else {
                    assigned
                        .entry(m.id.clone())
                        .or_insert_with(|| m.color.clone().unwrap_or_default());
                }
            }
        }
    }

    fn hours_per_weekday_mut(&mut self) -> impl Iterator<Item = &mut f32> {
        self.work.hours_per_weekday.iter_mut()
    }

    /// All members of all teams, each once (first occurrence wins).
    pub fn all_members(&self) -> Vec<Member> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for t in &self.teams {
            for m in &t.members {
                if seen.insert(m.id.clone()) {
                    out.push(m.clone());
                }
            }
        }
        out
    }

    pub fn team(&self, id: &str) -> Option<&Team> {
        self.teams.iter().find(|t| t.id == id)
    }

    /// Members for the given ids, in the given order. Unknown ids are skipped.
    pub fn members_by_ids(&self, ids: &[String]) -> Vec<Member> {
        let all = self.all_members();
        ids.iter()
            .filter_map(|id| all.iter().find(|m| &m.id == id).cloned())
            .collect()
    }
}

/// Turns whatever the user pasted into the REST base URL of the Jira instance.
///
/// * Jira Cloud (`*.atlassian.net`, `*.jira.com`): the API always lives at the origin, UI paths
///   such as `/jira/software/...` or `/jira/your-work` are dropped.
/// * Data Center: UI paths (`/browse/KEY`, `/secure/...`, `/projects/...`, `/issues/...`,
///   `/plugins/...`, `/rest/...`) are dropped, a context path like `/jira` is kept.
pub fn normalize_jira_url(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let mut url = url::Url::parse(&with_scheme)?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(Error::InvalidUrl("Nur http(s) wird unterstützt".into()));
    }
    let host = url
        .host_str()
        .ok_or_else(|| Error::InvalidUrl("Kein Host in der URL".into()))?
        .to_lowercase();
    url.set_query(None);
    url.set_fragment(None);
    let is_cloud = host.ends_with(".atlassian.net") || host.ends_with(".jira.com");
    let path = if is_cloud {
        String::new()
    } else {
        const UI_MARKERS: &[&str] = &[
            "/browse/",
            "/secure/",
            "/projects/",
            "/issues/",
            "/plugins/",
            "/rest/",
            "/login.jsp",
            "/jira/software/",
            "/servicedesk/",
        ];
        let mut p = url.path().to_string();
        let lower = p.to_lowercase();
        let cut = UI_MARKERS.iter().filter_map(|m| lower.find(m)).min();
        if let Some(i) = cut {
            p.truncate(i);
        }
        p.trim_end_matches('/').to_string()
    };
    url.set_path(&path);
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// Well-known locations for app data.
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| Error::Io("Kein Konfigurationsverzeichnis gefunden".into()))?
            .join(APP_DIR_NAME);
        let data_dir = dirs::data_dir()
            .ok_or_else(|| Error::Io("Kein Datenverzeichnis gefunden".into()))?
            .join(APP_DIR_NAME);
        Ok(Self {
            config_dir,
            data_dir,
        })
    }

    pub fn with_root(root: &Path) -> Self {
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
        }
    }

    pub fn ensure(&self) -> Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("yattat-cache.sqlite")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
}

pub fn load_settings(paths: &AppPaths) -> Result<Settings> {
    let file = paths.settings_file();
    if !file.exists() {
        return Ok(Settings::default());
    }
    let raw = std::fs::read_to_string(&file)?;
    let mut settings: Settings = serde_json::from_str(&raw)?;
    settings.tidy();
    Ok(settings)
}

pub fn save_settings(paths: &AppPaths, settings: &Settings) -> Result<()> {
    settings.validate()?;
    paths.ensure()?;
    let tmp = paths.settings_file().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(settings)?)?;
    std::fs::rename(&tmp, paths.settings_file())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, name: &str) -> Member {
        Member {
            id: id.into(),
            display_name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn roundtrip_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::with_root(dir.path());
        let mut s = Settings::default();
        s.jira.base_url = "https://jira.example.com/".into();
        s.teams.push(Team {
            id: "t1".into(),
            name: "Core".into(),
            members: vec![member("alice", "Alice"), member("bob", "Bob")],
        });
        s.tidy();
        save_settings(&paths, &s).unwrap();
        let loaded = load_settings(&paths).unwrap();
        assert_eq!(loaded, s);
        assert_eq!(loaded.jira_base(), "https://jira.example.com");
        assert_eq!(
            loaded.teams[0].members[0].color.as_deref(),
            Some(PALETTE[0])
        );
        assert_eq!(
            loaded.teams[0].members[1].color.as_deref(),
            Some(PALETTE[1])
        );
    }

    #[test]
    fn tidy_dedups_members_and_keeps_colors_per_member() {
        let mut s = Settings::default();
        let mut carol = member("carol", "Carol");
        carol.color = Some("#123456".into());
        s.teams.push(Team {
            id: "".into(),
            name: "  ".into(),
            members: vec![
                member("alice", "Alice"),
                member("alice", "Alice again"),
                carol.clone(),
                member("", "nobody"),
            ],
        });
        s.teams.push(Team {
            id: "t2".into(),
            name: "Zwei".into(),
            members: vec![member("alice", ""), member("carol", "Carol")],
        });
        s.work.hours_per_weekday[0] = 99.0;
        s.tidy();
        assert_eq!(s.teams[0].name, "Team 1");
        assert_eq!(s.teams[0].id, "team-1");
        assert_eq!(s.teams[0].members.len(), 2);
        let alice_color = s.teams[0].members[0].color.clone().unwrap();
        assert_eq!(
            s.teams[1].members[0].color.as_deref(),
            Some(alice_color.as_str())
        );
        assert_eq!(s.teams[1].members[0].display_name, "alice");
        // carol keeps her explicit color in team 1 and gets the same one in team 2
        assert_eq!(s.teams[0].members[1].color.as_deref(), Some("#123456"));
        assert_eq!(s.teams[1].members[1].color.as_deref(), Some("#123456"));
        assert_ne!(alice_color, "#123456");
        assert_eq!(s.work.hours_per_weekday[0], 24.0);
        assert_eq!(s.all_members().len(), 2);
        assert_eq!(
            s.members_by_ids(&["carol".into(), "x".into(), "alice".into()])
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            vec!["carol", "alice"]
        );
    }

    #[test]
    fn weekly_hours_are_validated() {
        let mut s = Settings::default();
        let mut m = member("a", "A");
        m.weekly_hours = Some(120.0);
        s.teams.push(Team {
            id: "t".into(),
            name: "T".into(),
            members: vec![m],
        });
        assert!(s.validate().is_err());
        s.teams[0].members[0].weekly_hours = Some(32.0);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn invalid_url_rejected() {
        let mut s = Settings::default();
        s.jira.base_url = "ftp://x".into();
        assert!(s.validate().is_err());
    }

    #[test]
    fn normalizes_cloud_and_dc_urls() {
        assert_eq!(
            normalize_jira_url("https://firma.atlassian.net/jira").unwrap(),
            "https://firma.atlassian.net"
        );
        assert_eq!(
            normalize_jira_url("https://x.atlassian.net/jira/software/projects/ABC/boards/1?x=1")
                .unwrap(),
            "https://x.atlassian.net"
        );
        assert_eq!(
            normalize_jira_url("x.atlassian.net").unwrap(),
            "https://x.atlassian.net"
        );
        assert_eq!(
            normalize_jira_url("https://jira.firma.de/browse/ABC-1").unwrap(),
            "https://jira.firma.de"
        );
        assert_eq!(
            normalize_jira_url("https://tools.firma.de/jira/secure/Dashboard.jspa").unwrap(),
            "https://tools.firma.de/jira"
        );
        assert_eq!(normalize_jira_url("  ").unwrap(), "");
        assert!(normalize_jira_url("ftp://jira.firma.de").is_err());
    }

    #[test]
    fn missing_fields_use_defaults() {
        let s: Settings =
            serde_json::from_str(r#"{"jira":{"baseUrl":"https://j.example"}}"#).unwrap();
        assert_eq!(s.refresh.interval_minutes, 30);
        assert!(s.teams.is_empty());
        assert_eq!(s.tempo.source, WorklogSource::JiraWorklogs);
        assert_eq!(s.tempo.cloud_api_base, "https://api.tempo.io/4");
        assert_eq!(s.work.pattern_hours_per_week(), 40.0);
    }
}
