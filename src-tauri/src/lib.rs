pub mod account_store;
pub mod commands;
pub mod download_tasks;
pub mod download_transfers;
pub mod state;

use std::sync::Arc;
use std::time::Duration;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, State, Wry};

use state::AppState;

const TRAY_ID: &str = "main-tray";

struct TrayMenuState {
    show: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

#[derive(serde::Deserialize)]
struct TrayLabelsPayload {
    show: String,
    quit: String,
    tooltip: String,
}

fn show_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn request_native_quit(app: &tauri::AppHandle) {
    show_main_window(app);
    if let Err(error) = app.emit("tray-quit-requested", ()) {
        log::warn!("[TRAY] could not notify the frontend to flush before exit: {error}");
        app.exit(0);
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(6)).await;
        log::warn!("[TRAY] frontend exit timed out; forcing application exit");
        handle.exit(0);
    });
}

fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<TrayMenuState> {
    let show = MenuItem::with_id(app, "show", "Open Cirrus", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Cirrus", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => request_native_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(TrayMenuState { show, quit })
}

#[tauri::command]
fn set_tray_labels(
    app: tauri::AppHandle,
    items: State<'_, TrayMenuState>,
    payload: TrayLabelsPayload,
) -> Result<(), String> {
    items.show.set_text(payload.show).map_err(|error| error.to_string())?;
    items.quit.set_text(payload.quit).map_err(|error| error.to_string())?;
    let tray = app.tray_by_id(TRAY_ID).ok_or_else(|| "system tray is unavailable".to_string())?;
    tray.set_tooltip(Some(payload.tooltip)).map_err(|error| error.to_string())?;
    log::info!("[TRAY] labels updated");
    Ok(())
}

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

            match setup_tray(app.handle()) {
                Ok(menu) => {
                    app.manage(menu);
                    state.set_tray_available(true);
                }
                Err(error) => log::warn!("[TRAY] could not create the system tray: {error}"),
            }

            state::spawn_cleanup_task(state);
            log::info!("Cirrus {} started", env!("CARGO_PKG_VERSION"));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::account::account_store_load,
            commands::account::account_store_save,
            commands::session::runtime_info,
            commands::session::tray_available,
            set_tray_labels,
            commands::session::quit_app,
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
