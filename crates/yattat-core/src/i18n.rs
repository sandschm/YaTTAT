//! Language setting and the messages the backend sends to the user interface.
//!
//! The frontend has its own dictionary for everything it renders; this module only covers texts
//! that originate in Rust (errors, login progress, status messages, export headers).

use serde::{Deserialize, Serialize};

/// The configured language. `System` follows the operating system's locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    #[default]
    System,
    De,
    En,
}

/// A concrete language for formatting messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

impl Language {
    pub fn resolve(self) -> Lang {
        match self {
            Language::De => Lang::De,
            Language::En => Lang::En,
            Language::System => system_lang(),
        }
    }
}

/// German when the system locale starts with `de`, English otherwise.
pub fn system_lang() -> Lang {
    let german = sys_locale::get_locale()
        .map(|l| l.to_lowercase().starts_with("de"))
        .unwrap_or(false);
    if german {
        Lang::De
    } else {
        Lang::En
    }
}

/// All message keys; the test below checks that both languages define each one.
pub const KEYS: &[&str] = &[
    "notLoggedIn",
    "sessionExpired",
    "jiraUrlChanged",
    "noJiraUrl",
    "invalidJiraUrl",
    "loginRunning",
    "loginWindowTitle",
    "loginWindowFailed",
    "waitingForLogin",
    "loginCancelled",
    "loginTimeout",
    "sessionDetected",
    "noSessionYet",
    "cookiesNotAccepted",
    "myself404",
    "probeFailed",
    "loginNotAccepted",
    "storedSessionInvalid",
    "credentialsUnreadable",
    "unknownAuthMethod",
    "unknownFormat",
    "tempoTokenRejected",
    "tempoTokenMissing",
    "compareNeedsTempo",
    "noMembers",
];

/// Message template by key. Placeholders `{0}`, `{1}` are filled by [`tf`].
pub fn t(lang: Lang, key: &str) -> &'static str {
    use Lang::*;
    match (key, lang) {
        ("notLoggedIn", De) => "Nicht bei Jira angemeldet",
        ("notLoggedIn", En) => "Not signed in to Jira",
        ("sessionExpired", De) => "Die Jira-Sitzung ist abgelaufen. Bitte erneut anmelden.",
        ("sessionExpired", En) => "The Jira session has expired. Please sign in again.",
        ("jiraUrlChanged", De) => "Jira-URL geändert. Bitte erneut anmelden.",
        ("jiraUrlChanged", En) => "Jira URL changed. Please sign in again.",
        ("noJiraUrl", De) => "Bitte zuerst die Jira-URL eintragen",
        ("noJiraUrl", En) => "Please enter the Jira URL first",
        ("invalidJiraUrl", De) => "Ungültige Jira-URL: {0}",
        ("invalidJiraUrl", En) => "Invalid Jira URL: {0}",
        ("loginRunning", De) => "Eine Anmeldung läuft bereits",
        ("loginRunning", En) => "A login is already in progress",
        ("loginWindowTitle", De) => "Bei Jira anmelden",
        ("loginWindowTitle", En) => "Sign in to Jira",
        ("loginWindowFailed", De) => "Login-Fenster konnte nicht geöffnet werden: {0}",
        ("loginWindowFailed", En) => "The login window could not be opened: {0}",
        ("waitingForLogin", De) => "Warte auf Anmeldung im Fenster …",
        ("waitingForLogin", En) => "Waiting for the login in the window …",
        ("loginCancelled", De) => "Anmeldung abgebrochen. Letzte Prüfung: {0}",
        ("loginCancelled", En) => "Login cancelled. Last check: {0}",
        ("loginTimeout", De) => "Zeitüberschreitung bei der Anmeldung. Letzte Prüfung: {0}",
        ("loginTimeout", En) => "Login timed out. Last check: {0}",
        ("sessionDetected", De) => "Sitzung erkannt: {0}",
        ("sessionDetected", En) => "Session detected: {0}",
        ("noSessionYet", De) => "noch keine Sitzung (HTTP {0})",
        ("noSessionYet", En) => "no session yet (HTTP {0})",
        ("cookiesNotAccepted", De) => {
            "Sitzungs-Cookies vorhanden ({0}), Jira akzeptiert sie noch nicht (HTTP {1})"
        }
        ("cookiesNotAccepted", En) => {
            "session cookies present ({0}), Jira does not accept them yet (HTTP {1})"
        }
        ("myself404", De) => {
            "{0}/rest/api/2/myself antwortet 404 – die Jira-URL ist vermutlich falsch (bei Jira Cloud nur https://<firma>.atlassian.net)"
        }
        ("myself404", En) => {
            "{0}/rest/api/2/myself answers 404 – the Jira URL is probably wrong (Jira Cloud: only https://<company>.atlassian.net)"
        }
        ("probeFailed", De) => "Prüfung fehlgeschlagen: {0}",
        ("probeFailed", En) => "Check failed: {0}",
        ("loginNotAccepted", De) => "Anmeldung nicht akzeptiert (HTTP {0})",
        ("loginNotAccepted", En) => "Login not accepted (HTTP {0})",
        ("storedSessionInvalid", De) => {
            "Gespeicherte Sitzung ist nicht mehr gültig. Bitte erneut anmelden."
        }
        ("storedSessionInvalid", En) => "The stored session is no longer valid. Please sign in again.",
        ("credentialsUnreadable", De) => "Zugangsdaten konnten nicht gelesen werden: {0}",
        ("credentialsUnreadable", En) => "Credentials could not be read: {0}",
        ("unknownAuthMethod", De) => "Unbekannte Anmeldeart {0}",
        ("unknownAuthMethod", En) => "Unknown login method {0}",
        ("unknownFormat", De) => "Unbekanntes Format {0}",
        ("unknownFormat", En) => "Unknown format {0}",
        ("tempoTokenRejected", De) => "Tempo-Token wurde nicht akzeptiert: {0}",
        ("tempoTokenRejected", En) => "Tempo token was not accepted: {0}",
        ("tempoTokenMissing", De) => {
            "Tempo Cloud ist als Quelle gewählt, aber es ist kein Tempo-API-Token gespeichert (Einstellungen → Verbindung)."
        }
        ("tempoTokenMissing", En) => {
            "Tempo Cloud is selected as source, but no Tempo API token is stored (Settings → Connection)."
        }
        ("compareNeedsTempo", De) => {
            "Für den Abgleich wird ein Tempo-API-Token benötigt (Einstellungen → Verbindung → Quelle der Zeiten)."
        }
        ("compareNeedsTempo", En) => {
            "The comparison needs a Tempo API token (Settings → Connection → Source of times)."
        }
        ("noMembers", De) => "Keine Mitglieder im Team",
        ("noMembers", En) => "No members in the team",
        _ => "",
    }
}

/// [`t`] with `{0}`, `{1}`, … replaced by `args`.
pub fn tf(lang: Lang, key: &str, args: &[&str]) -> String {
    let mut out = t(lang, key).to_string();
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_both_languages() {
        for key in KEYS {
            assert!(!t(Lang::De, key).is_empty(), "missing de: {key}");
            assert!(!t(Lang::En, key).is_empty(), "missing en: {key}");
        }
        assert_eq!(t(Lang::En, "nope"), "");
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(
            tf(Lang::En, "cookiesNotAccepted", &["JSESSIONID", "401"]),
            "session cookies present (JSESSIONID), Jira does not accept them yet (HTTP 401)"
        );
        assert_eq!(tf(Lang::De, "noMembers", &[]), "Keine Mitglieder im Team");
    }

    #[test]
    fn language_resolves() {
        assert_eq!(Language::De.resolve(), Lang::De);
        assert_eq!(Language::En.resolve(), Lang::En);
        let _ = Language::System.resolve();
    }
}
