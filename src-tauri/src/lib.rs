//! YaTTAT desktop application (Tauri 2). All business logic lives in `yattat-core`.

mod commands;
mod jobs;
mod login;
mod state;

use state::AppState;
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Manager};
use yattat_core::settings::AppPaths;

pub use jobs::emit;

static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

fn init_logging(paths: &AppPaths) {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
    let log_dir = paths.log_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "yattat.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    let _ = LOG_GUARD.set(guard);
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,yattat=debug,yattat_core=debug"));
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_ansi(false).with_writer(file_writer));
    #[cfg(debug_assertions)]
    let registry = registry.with(fmt::layer().with_writer(std::io::stderr));
    let _ = registry.try_init();
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = AppPaths::discover().expect("Benutzerverzeichnisse nicht gefunden");
    init_logging(&paths);
    let state = Arc::new(
        AppState::init(paths).expect("Anwendungsdaten konnten nicht initialisiert werden"),
    );
    let state_for_setup = state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(move |app| {
            let handle = app.handle().clone();
            let state = state_for_setup;
            {
                let h = handle.clone();
                let s = state.clone();
                tauri::async_runtime::spawn(async move { jobs::refresh_loop(h, s).await });
            }
            // Developer aid: `YATTAT_DEV_LOGIN=1 npm run tauri dev` opens the browser login right
            // away so the SSO flow can be exercised without clicking through the UI.
            if cfg!(debug_assertions) && std::env::var_os("YATTAT_DEV_LOGIN").is_some() {
                let h = handle.clone();
                let s = state.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    match login::browser_login(h, s).await {
                        Ok(status) => tracing::info!(
                            "dev login finished: {:?}",
                            status.user.map(|u| u.display_name)
                        ),
                        Err(e) => tracing::warn!("dev login ended: {e}"),
                    }
                });
            }
            jobs::load_tempo_cloud(&state);
            tauri::async_runtime::spawn(async move {
                jobs::restore_session(&handle, &state).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_auth_status,
            commands::start_browser_login,
            commands::cancel_browser_login,
            commands::login_with_password,
            commands::login_with_token,
            commands::logout,
            commands::search_projects,
            commands::list_projects,
            commands::set_project_selection,
            commands::set_project_color,
            commands::search_users,
            commands::fetch_worklogs,
            commands::get_fetch_status,
            commands::get_coverage,
            commands::get_stats,
            commands::list_worklogs,
            commands::export_worklogs,
            commands::get_issue_url,
            commands::clear_cache,
            commands::set_tempo_cloud_token,
            commands::compare_sources,
            commands::get_app_info,
        ])
        .run(tauri::generate_context!())
        .expect("Fehler beim Start von YaTTAT");
}
