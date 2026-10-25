pub mod account_store;
pub mod commands;
pub mod download_tasks;
pub mod download_transfers;
pub mod state;

use std::sync::Arc;

use tauri::Manager;

use state::AppState;

pub fn run() {
    env_logger_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = match app.path().app_data_dir() {
                Ok(dir) => Arc::new(AppState::new(&dir)),
                Err(error) => {
                    log::warn!(
                        "[VAULT] could not resolve app data directory; persistence is disabled for this run: {error}"
                    );
                    Arc::new(AppState::without_persistence())
                }
            };
            app.manage(Arc::clone(&state));

            state::spawn_cleanup_task(state);
            log::info!("Cirrus {} started", env!("CARGO_PKG_VERSION"));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::account::account_store_load,
            commands::account::account_store_save,
            commands::session::runtime_info,
            commands::photos::download_set_speed_limit,
            commands::photos::download_cancel_transfer,
            commands::photos::open_download_folder,
            commands::download_tasks::download_tasks_load,
            commands::download_tasks::download_tasks_save,
            commands::download_tasks::download_tasks_rekey,
            commands::download_tasks::download_tasks_delete_account,
            commands::devices::refresh_devices,
            commands::devices::play_sound,
            commands::session::session_state,
            commands::session::session_avatar,
            commands::session::session_logout,
            commands::photos::photos_login,
            commands::photos::photos_verify_2fa,
            commands::photos::photos_finish_2fa,
            commands::photos::photos_send_code,
            commands::photos::photos_list,
            commands::photos::photos_total,
            commands::photos::photos_asset_distribution,
            commands::photos::photos_month_exact,
            commands::photos::photos_status,
            commands::photos::photos_check_approval,
            commands::photos::photos_2fa_options,
            commands::photos::photos_2fa_sms,
            commands::photos::photos_verify_2fa_phone,
            commands::photos::save_photo,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Tauri application");
}

fn env_logger_init() {
    struct StderrLogger;

    impl log::Log for StderrLogger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Info
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                eprintln!("[{}] {}", record.level(), record.args());
            }
        }

        fn flush(&self) {}
    }

    static LOGGER: StderrLogger = StderrLogger;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
}
