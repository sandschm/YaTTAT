//! YaTTAT core: everything that is not UI. No Tauri dependency, fully testable with `cargo test`.
//!
//! The app only reads from Jira: it fetches the worklogs of the configured team members into a
//! local SQLite cache and computes statistics from there.

pub mod error;
pub mod export;
pub mod fetch;
pub mod i18n;
pub mod jira;
pub mod models;
pub mod secrets;
pub mod settings;
pub mod stats;
pub mod storage;
pub mod tempo;

pub use error::{Error, Result};
pub use i18n::{Lang, Language};
pub use settings::TimeFormat;

/// Human readable duration: `1:05` (hours:minutes) or `1,08` / `1.08` (decimal hours, separator
/// by language).
pub fn format_duration(seconds: i64, format: TimeFormat, lang: Lang) -> String {
    let seconds = seconds.max(0);
    match format {
        TimeFormat::HoursMinutes => format!("{}:{:02}", seconds / 3600, (seconds % 3600) / 60),
        TimeFormat::Decimal => {
            let s = format!("{:.2}", seconds as f64 / 3600.0);
            match lang {
                Lang::De => s.replace('.', ","),
                Lang::En => s,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(
            format_duration(3900, TimeFormat::HoursMinutes, Lang::De),
            "1:05"
        );
        assert_eq!(format_duration(3900, TimeFormat::Decimal, Lang::De), "1,08");
        assert_eq!(format_duration(3900, TimeFormat::Decimal, Lang::En), "1.08");
        assert_eq!(
            format_duration(-5, TimeFormat::HoursMinutes, Lang::En),
            "0:00"
        );
    }
}
