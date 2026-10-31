//! Error type of the core. `Display` is English (logs, tests); [`Error::localized`] gives the
//! message for the user in the configured language.

use crate::i18n::Lang;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Not signed in")]
    NotAuthenticated,
    #[error("Session expired or no permission (HTTP {0})")]
    AuthExpired(u16),
    #[error("Jira answered HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("Network error: {0}")]
    Network(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Only http(s) is supported")]
    UnsupportedScheme,
    #[error("The URL has no host")]
    NoHost,
    #[error("Database error: {0}")]
    Database(String),
    #[error("Keyring: {0}")]
    Keyring(String),
    #[error("Stored credentials could not be decrypted (key changed?)")]
    DecryptFailed,
    #[error("Encryption failed")]
    EncryptFailed,
    #[error("Master key invalid: {0}")]
    MasterKeyInvalid(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("No configuration directory found")]
    NoConfigDir,
    #[error("No data directory found")]
    NoDataDir,
    #[error("Invalid data: {0}")]
    Parse(String),
    #[error("Invalid date: {0}")]
    InvalidDate(String),
    #[error("User response incomplete")]
    IncompleteUser,
    #[error("Issue {0} could not be read")]
    IssueUnreadable(String),
    #[error("No session cookies in the response")]
    NoSessionCookies,
    #[error("Wrong username or password")]
    WrongCredentials,
    #[error("Jira asks for a CAPTCHA. Sign in once in the browser or use the browser login.")]
    CaptchaRequired,
    #[error("Login failed (HTTP {0})")]
    LoginFailed(u16),
    #[error("Tempo API token not accepted or missing permission (HTTP {0})")]
    TempoAuth(u16),
    #[error("Tempo answered HTTP {status}: {message}")]
    TempoHttp { status: u16, message: String },
    #[error("Tempo API base is not a valid URL")]
    InvalidTempoBase,
    #[error("Refresh interval must be at least 1 minute")]
    IntervalTooSmall,
    #[error("Weekly hours of {0} must be between 0 and 80")]
    WeeklyHoursOutOfRange(String),
    #[error("{0}")]
    Invalid(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Message for the user in the given language.
    pub fn localized(&self, lang: Lang) -> String {
        if lang == Lang::En {
            return self.to_string();
        }
        match self {
            Error::NotAuthenticated => "Nicht angemeldet".into(),
            Error::AuthExpired(code) => {
                format!("Sitzung abgelaufen oder keine Berechtigung (HTTP {code})")
            }
            Error::Http { status, message } => {
                format!("Jira antwortete mit HTTP {status}: {message}")
            }
            Error::Network(e) => format!("Netzwerkfehler: {e}"),
            Error::InvalidUrl(u) => format!("Ungültige URL: {u}"),
            Error::UnsupportedScheme => "Nur http(s) wird unterstützt".into(),
            Error::NoHost => "Kein Host in der URL".into(),
            Error::Database(e) => format!("Datenbankfehler: {e}"),
            Error::Keyring(e) => format!("Schlüsselbund: {e}"),
            Error::DecryptFailed => {
                "Gespeicherte Zugangsdaten konnten nicht entschlüsselt werden (Schlüssel geändert?)"
                    .into()
            }
            Error::EncryptFailed => "Verschlüsselung fehlgeschlagen".into(),
            Error::MasterKeyInvalid(e) => format!("Master-Key ungültig: {e}"),
            Error::Io(e) => format!("Ein-/Ausgabefehler: {e}"),
            Error::NoConfigDir => "Kein Konfigurationsverzeichnis gefunden".into(),
            Error::NoDataDir => "Kein Datenverzeichnis gefunden".into(),
            Error::Parse(e) => format!("Ungültige Daten: {e}"),
            Error::InvalidDate(d) => format!("Ungültiges Datum: {d}"),
            Error::IncompleteUser => "Benutzerantwort unvollständig".into(),
            Error::IssueUnreadable(key) => format!("Ticket {key} konnte nicht gelesen werden"),
            Error::NoSessionCookies => "Keine Session-Cookies in der Antwort".into(),
            Error::WrongCredentials => "Benutzername oder Passwort falsch".into(),
            Error::CaptchaRequired => {
                "Jira verlangt ein CAPTCHA. Bitte einmal im Browser anmelden oder die Browser-Anmeldung verwenden."
                    .into()
            }
            Error::LoginFailed(code) => format!("Anmeldung fehlgeschlagen (HTTP {code})"),
            Error::TempoAuth(code) => format!(
                "Tempo-API-Token nicht akzeptiert oder keine Berechtigung (HTTP {code})"
            ),
            Error::TempoHttp { status, message } => {
                format!("Tempo antwortete mit HTTP {status}: {message}")
            }
            Error::InvalidTempoBase => "Tempo-API-Basis ist keine gültige URL".into(),
            Error::IntervalTooSmall => {
                "Aktualisierungsintervall muss mindestens 1 Minute sein".into()
            }
            Error::WeeklyHoursOutOfRange(name) => {
                format!("Wochenstunden von {name} müssen zwischen 0 und 80 liegen")
            }
            Error::Invalid(m) => m.clone(),
            Error::NotFound(w) => format!("Nicht gefunden: {w}"),
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Database(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if e.is_decode() {
            Error::Parse(e.to_string())
        } else {
            Error::Network(e.to_string())
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Parse(e.to_string())
    }
}

impl From<url::ParseError> for Error {
    fn from(e: url::ParseError) -> Self {
        Error::InvalidUrl(e.to_string())
    }
}

impl From<keyring::Error> for Error {
    fn from(e: keyring::Error) -> Self {
        Error::Keyring(e.to_string())
    }
}

impl From<csv::Error> for Error {
    fn from(e: csv::Error) -> Self {
        Error::Io(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_messages() {
        let e = Error::AuthExpired(401);
        assert_eq!(
            e.localized(Lang::En),
            "Session expired or no permission (HTTP 401)"
        );
        assert_eq!(
            e.localized(Lang::De),
            "Sitzung abgelaufen oder keine Berechtigung (HTTP 401)"
        );
        assert_eq!(
            Error::WeeklyHoursOutOfRange("Sam".into()).localized(Lang::De),
            "Wochenstunden von Sam müssen zwischen 0 und 80 liegen"
        );
    }
}
