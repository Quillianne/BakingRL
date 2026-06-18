pub mod bus;
pub mod ingestor;
pub mod models;
pub mod plugin_host;
pub mod plugin_package;
pub mod registry;

use crate::bus::{BusEvent, EventBus};
use crate::ingestor::{start_tcp_ingestor, TelemetryStatusState};
use crate::models::{GameEvent, TelemetryConnectionStatus};
use crate::plugin_host::{
    call_service_export, clear_plugin_diagnostics, delete_package_secret, discard_prepared_package,
    get_app_settings, get_package_configuration_state, get_package_settings, get_runtime_info,
    inspect_package_bundle, install_package_from_file, install_package_from_url,
    install_prepared_package, list_packages, list_plugin_diagnostics, open_package_configuration,
    open_package_secrets, open_package_webview, packages_dir, plugin_registry_get,
    prepare_package_from_deep_link, prepare_package_from_git, prepare_package_from_url,
    read_package_file_text, reload_packages, remove_package, save_app_settings,
    save_package_settings, set_package_enabled, set_package_secret, PluginHost,
};
use crate::registry::{registry_entries, registry_get, Registry};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

const PACKAGE_FILE_OPENED_EVENT: &str = "bakingrl-package-files-opened";
#[cfg(desktop)]
const TRAY_MENU_SHOW_ID: &str = "bakingrl-tray-show";
#[cfg(desktop)]
const TRAY_MENU_QUIT_ID: &str = "bakingrl-tray-quit";

#[derive(Default)]
struct PendingPackageFileOpens(Mutex<Vec<String>>);

impl PendingPackageFileOpens {
    fn new(paths: Vec<String>) -> Self {
        Self(Mutex::new(paths))
    }
}

fn normalize_package_file_path(path: PathBuf) -> Option<String> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("brlp"))
    {
        return None;
    }

    Some(
        std::fs::canonicalize(&path)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string(),
    )
}

fn package_file_path_from_arg(value: &str, cwd: Option<&Path>) -> Option<String> {
    let trimmed = value.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(url) = url::Url::parse(trimmed) {
        if url.scheme() == "file" {
            return url
                .to_file_path()
                .ok()
                .and_then(normalize_package_file_path);
        }
        return None;
    }

    let path = PathBuf::from(trimmed);
    let path = if path.is_absolute() {
        path
    } else if let Some(cwd) = cwd {
        cwd.join(path)
    } else {
        path
    };
    normalize_package_file_path(path)
}

fn collect_package_file_paths<'a>(
    values: impl IntoIterator<Item = &'a str>,
    cwd: Option<&Path>,
) -> Vec<String> {
    let mut paths = Vec::new();
    for value in values {
        if let Some(path) = package_file_path_from_arg(value, cwd) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
fn collect_package_file_paths_from_urls(urls: &[url::Url]) -> Vec<String> {
    let mut paths = Vec::new();
    for url in urls {
        if url.scheme() != "file" {
            continue;
        }
        if let Ok(path) = url.to_file_path() {
            if let Some(path) = normalize_package_file_path(path) {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(main_window) = app.get_webview_window("main") {
        let _ = main_window.show();
        let _ = main_window.unminimize();
        let _ = main_window.set_focus();
    }
}

#[cfg(desktop)]
fn create_system_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};

    let show = MenuItem::with_id(
        app,
        TRAY_MENU_SHOW_ID,
        "Afficher BakingRL",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        TRAY_MENU_QUIT_ID,
        "Quitter BakingRL",
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("BakingRL")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id() == TRAY_MENU_SHOW_ID {
                show_main_window(app);
            } else if event.id() == TRAY_MENU_QUIT_ID {
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => show_main_window(tray.app_handle()),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    Ok(())
}

fn enqueue_package_file_opens(app: &tauri::AppHandle, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }

    if let Some(pending) = app.try_state::<PendingPackageFileOpens>() {
        let mut queued = pending.0.lock().unwrap();
        for path in paths {
            if !queued.contains(&path) {
                queued.push(path);
            }
        }
    }

    show_main_window(app);
    if let Err(err) = app.emit(PACKAGE_FILE_OPENED_EVENT, ()) {
        warn!("Unable to emit package file open event: {}", err);
    }
}

#[tauri::command]
fn get_telemetry_status(
    status_state: tauri::State<'_, TelemetryStatusState>,
) -> TelemetryConnectionStatus {
    status_state.lock().unwrap().clone()
}

#[tauri::command]
fn get_telemetry_snapshot(bus: tauri::State<'_, Arc<EventBus>>) -> Option<GameEvent> {
    bus.latest_game_event().as_deref().cloned()
}

#[tauri::command]
fn emit_developer_telemetry(
    bus: tauri::State<'_, Arc<EventBus>>,
    frame: serde_json::Value,
) -> Result<(), String> {
    let event_name = frame
        .get("Event")
        .and_then(|event| event.as_str())
        .map(str::trim)
        .filter(|event| !event.is_empty())
        .ok_or_else(|| "Telemetry frame must contain a non-empty string Event field.".to_string())?
        .to_string();
    let data = frame
        .get("Data")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    bus.publish(BusEvent::GameData(Arc::new(GameEvent {
        event: event_name,
        data,
    })));
    Ok(())
}

#[tauri::command]
fn take_pending_package_file_opens(
    pending: tauri::State<'_, PendingPackageFileOpens>,
) -> Vec<String> {
    let mut pending = pending.0.lock().unwrap();
    std::mem::take(&mut *pending)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Erreur lors de l'initialisation de tracing");

    let mut builder = tauri::Builder::default();
    let current_dir = env::current_dir().ok();
    let launch_package_file_paths = collect_package_file_paths(
        env::args().collect::<Vec<_>>().iter().map(String::as_str),
        current_dir.as_deref(),
    );
    let launched_from_package_file = !launch_package_file_paths.is_empty();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            if let Some(deep_link) = app.try_state::<tauri_plugin_deep_link::DeepLink<tauri::Wry>>()
            {
                deep_link.handle_cli_arguments(argv.iter());
            }
            let cwd = PathBuf::from(cwd);
            let package_file_paths =
                collect_package_file_paths(argv.iter().map(String::as_str), Some(cwd.as_path()));
            enqueue_package_file_opens(app, package_file_paths);
            show_main_window(app);
        }));
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |_app| {
            info!("Démarrage du moteur BakingRL...");

            let app_handle = _app.handle().clone();

            #[cfg(desktop)]
            if let Err(err) = create_system_tray(_app) {
                warn!("Unable to create system tray icon: {}", err);
            }

            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(err) = _app.deep_link().register_all() {
                    warn!("Unable to register deep-link schemes: {}", err);
                }
            }

            let bus = Arc::new(EventBus::new(1024));
            let registry = Arc::new(Registry::new());
            _app.manage(bus.clone());
            _app.manage(registry.clone());
            _app.manage(PendingPackageFileOpens::new(launch_package_file_paths));

            let plugin_host = Arc::new(
                PluginHost::new(app_handle.clone(), bus.clone(), registry.clone())
                    .expect("Impossible d'initialiser le gestionnaire de packages"),
            );
            plugin_host.initialize();
            _app.manage(plugin_host.clone());

            if plugin_host.get_app_settings().behavior.start_minimized
                && !launched_from_package_file
            {
                if let Some(main_window) = _app.get_webview_window("main") {
                    let _ = main_window.hide();
                }
            }

            let telemetry = plugin_host.get_app_settings().telemetry;
            let telemetry_status = Arc::new(Mutex::new(TelemetryConnectionStatus::new(
                "connecting",
                Some(format!(
                    "Connecting to {}:{}",
                    telemetry.rocket_league_host, telemetry.rocket_league_port
                )),
                telemetry.rocket_league_host.clone(),
                telemetry.rocket_league_port,
            )));
            _app.manage(telemetry_status.clone());

            let ingestor_bus = bus.clone();
            let ingestor_app_handle = app_handle.clone();
            let ingestor_plugin_host = plugin_host.clone();
            tauri::async_runtime::spawn(async move {
                start_tcp_ingestor(
                    ingestor_bus,
                    ingestor_app_handle,
                    telemetry_status,
                    ingestor_plugin_host,
                    telemetry.rocket_league_host,
                    telemetry.rocket_league_port,
                )
                .await;
            });

            let mut rx = bus.subscribe();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                while let Ok(event) = rx.recv().await {
                    if let BusEvent::GameData(data) = event {
                        if let Err(e) = app_handle.emit("bakingrl-telemetry", &(*data)) {
                            tracing::warn!("Failed to emit telemetry via IPC: {}", e);
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_packages,
            inspect_package_bundle,
            install_package_from_file,
            install_package_from_url,
            prepare_package_from_url,
            prepare_package_from_deep_link,
            prepare_package_from_git,
            install_prepared_package,
            discard_prepared_package,
            packages_dir,
            get_runtime_info,
            list_plugin_diagnostics,
            clear_plugin_diagnostics,
            get_app_settings,
            save_app_settings,
            get_package_settings,
            save_package_settings,
            get_package_configuration_state,
            set_package_secret,
            delete_package_secret,
            set_package_enabled,
            remove_package,
            reload_packages,
            read_package_file_text,
            call_service_export,
            plugin_registry_get,
            open_package_webview,
            open_package_configuration,
            open_package_secrets,
            registry_get,
            registry_entries,
            get_telemetry_status,
            get_telemetry_snapshot,
            emit_developer_telemetry,
            take_pending_package_file_opens,
        ])
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if window.label() == "main" {
                    let plugin_host = window.state::<Arc<PluginHost>>();
                    if plugin_host.get_app_settings().behavior.close_will_hide {
                        api.prevent_close();
                        let _ = window.hide();
                    } else {
                        window.app_handle().exit(0);
                    }
                }
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("Erreur lors de l'exécution de l'application Tauri")
        .run(|_app, _event| {
            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
            if let tauri::RunEvent::Opened { urls } = _event {
                enqueue_package_file_opens(_app, collect_package_file_paths_from_urls(&urls));
            }
        });
}
