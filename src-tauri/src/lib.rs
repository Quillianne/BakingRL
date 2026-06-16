pub mod bus;
pub mod ingestor;
pub mod models;
pub mod obs_gateway;
pub mod plugin_host;
pub mod plugin_package;
pub mod registry;
pub mod window_watcher;

use crate::bus::{BusEvent, EventBus};
use crate::ingestor::{start_tcp_ingestor, TelemetryStatusState};
use crate::models::{GameEvent, ObsGatewayStatus, TelemetryConnectionStatus};
use crate::plugin_host::{
    call_service_export, clear_plugin_diagnostics, create_overlay_layout, create_page,
    delete_overlay_layout, delete_package_secret, delete_page, discard_prepared_package,
    duplicate_overlay_layout, duplicate_page, get_app_settings, get_marketplace_catalog,
    get_overlay_layouts, get_package_configuration_page, get_package_configuration_state,
    get_package_settings, get_pages, get_runtime_info, get_visual_settings_schema,
    import_package_layout, import_package_page, inspect_package_bundle, install_package_from_file,
    install_package_from_url, install_prepared_package, list_packages, list_plugin_diagnostics,
    open_package_configuration, open_package_secrets, open_page, packages_dir, plugin_registry_get,
    prepare_marketplace_package, prepare_package_from_deep_link, prepare_package_from_git,
    prepare_package_from_url, read_visual_export_source, refresh_marketplace, reload_packages,
    remove_package, save_app_settings, save_overlay_layout, save_package_settings, save_page,
    set_active_overlay_layout, set_package_enabled, set_package_secret, set_stream_overlay_layout,
    PluginHost,
};
use crate::registry::{registry_entries, registry_get, Registry};
use crate::window_watcher::start_window_visibility_watcher;
use std::env;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{
    Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

const INGAME_OVERLAY_LABEL: &str = "overlay-ingame";
const EDITOR_OVERLAY_LABEL: &str = "overlay-editor";
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

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlayMonitorDescriptor {
    id: String,
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_factor: f64,
    primary: bool,
    current: bool,
}

fn monitor_id(monitor: &Monitor) -> String {
    if let Some(name) = monitor.name() {
        if !name.trim().is_empty() {
            return format!("name:{name}");
        }
    }

    let position = monitor.position();
    let size = monitor.size();
    format!(
        "rect:{}:{}:{}:{}",
        position.x, position.y, size.width, size.height
    )
}

fn same_monitor(a: &Monitor, b: &Monitor) -> bool {
    a.position() == b.position() && a.size() == b.size()
}

fn monitor_matches_setting(monitor: &Monitor, setting: &str) -> bool {
    monitor_id(monitor) == setting || monitor.name().is_some_and(|name| name == setting)
}

fn place_editor_like_overlay(
    app: &tauri::AppHandle,
    editor_window: &WebviewWindow,
    plugin_host: &PluginHost,
) {
    if let Some(overlay_window) = app.get_webview_window(INGAME_OVERLAY_LABEL) {
        if let Ok(position) = overlay_window.outer_position() {
            let _ = editor_window.set_position(position);
        }
        if let Ok(size) = overlay_window.inner_size() {
            let _ = editor_window.set_size(size);
        }
        return;
    }

    let settings = plugin_host.get_app_settings();
    if settings.overlay.use_monitor_size {
        let selected_monitor = settings
            .overlay
            .monitor_id
            .as_deref()
            .and_then(|monitor_id| {
                editor_window
                    .available_monitors()
                    .ok()?
                    .into_iter()
                    .find(|monitor| monitor_matches_setting(monitor, monitor_id))
            });
        if let Some(monitor) = selected_monitor
            .or_else(|| editor_window.current_monitor().ok().flatten())
            .or_else(|| editor_window.primary_monitor().ok().flatten())
        {
            let position = monitor.position();
            let size = monitor.size();
            let _ = editor_window.set_position(PhysicalPosition::new(position.x, position.y));
            let _ = editor_window.set_size(PhysicalSize::new(size.width, size.height));
            return;
        }
    }

    let _ = editor_window.set_size(PhysicalSize::new(
        settings.overlay.screen_width.max(1),
        settings.overlay.screen_height.max(1),
    ));
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
fn open_overlay_layout_editor(
    app: tauri::AppHandle,
    plugin_host: tauri::State<'_, Arc<PluginHost>>,
    layout_id: String,
) -> Result<(), String> {
    let overlay_layouts = plugin_host.get_overlay_layouts();
    if !overlay_layouts
        .layouts
        .iter()
        .any(|layout| layout.id == layout_id)
    {
        return Err(format!("Layout '{layout_id}' was not found."));
    }

    if let Some(overlay_window) = app.get_webview_window(INGAME_OVERLAY_LABEL) {
        let _ = overlay_window.hide();
    }

    if let Some(editor_window) = app.get_webview_window(EDITOR_OVERLAY_LABEL) {
        place_editor_like_overlay(&app, &editor_window, &plugin_host);
        let _ = editor_window.set_ignore_cursor_events(false);
        let path = format!("/editor/layout/{layout_id}");
        let js_path = serde_json::to_string(&path).map_err(|error| error.to_string())?;
        editor_window
            .eval(format!("window.location.href = {js_path};"))
            .map_err(|error| error.to_string())?;
        let _ = editor_window.show();
        let _ = editor_window.set_focus();
        return Ok(());
    }

    let editor_window = WebviewWindowBuilder::new(
        &app,
        EDITOR_OVERLAY_LABEL,
        WebviewUrl::App(PathBuf::from(format!("/editor/layout/{layout_id}"))),
    )
    .title("BakingRL Layout Editor")
    .transparent(true)
    .decorations(false)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .build()
    .map_err(|error| error.to_string())?;

    place_editor_like_overlay(&app, &editor_window, &plugin_host);
    let _ = editor_window.set_ignore_cursor_events(false);
    let _ = editor_window.show();
    let _ = editor_window.set_focus();
    Ok(())
}

#[tauri::command]
fn close_overlay_editor(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(editor_window) = app.get_webview_window(EDITOR_OVERLAY_LABEL) {
        editor_window.close().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_telemetry_status(
    status_state: tauri::State<'_, TelemetryStatusState>,
) -> TelemetryConnectionStatus {
    status_state.lock().unwrap().clone()
}

#[tauri::command]
fn get_obs_gateway_status(plugin_host: tauri::State<'_, Arc<PluginHost>>) -> ObsGatewayStatus {
    let obs_settings = plugin_host.get_app_settings().obs;
    let address = format!("{}:{}", obs_settings.host, obs_settings.port);
    let connect_host = match obs_settings.host.as_str() {
        "0.0.0.0" | "::" => "127.0.0.1",
        host => host,
    };
    let connect_address = format!("{}:{}", connect_host, obs_settings.port);
    let running = connect_address
        .to_socket_addrs()
        .ok()
        .into_iter()
        .flatten()
        .any(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(120)).is_ok());

    ObsGatewayStatus {
        running,
        address,
        message: if running {
            None
        } else {
            Some("OBS gateway is not accepting connections.".to_string())
        },
    }
}

#[tauri::command]
fn list_overlay_monitors(app: tauri::AppHandle) -> Result<Vec<OverlayMonitorDescriptor>, String> {
    let reference_window = app
        .get_webview_window(INGAME_OVERLAY_LABEL)
        .or_else(|| app.get_webview_window("main"))
        .ok_or_else(|| "No window is available to list monitors.".to_string())?;
    let monitors = reference_window
        .available_monitors()
        .map_err(|error| error.to_string())?;
    let primary = reference_window.primary_monitor().ok().flatten();
    let current = reference_window.current_monitor().ok().flatten();

    Ok(monitors
        .into_iter()
        .map(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            let fallback_name = format!(
                "Display {}x{} at {},{}",
                size.width, size.height, position.x, position.y
            );
            let name = monitor
                .name()
                .map(ToString::to_string)
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(fallback_name);
            let primary = primary
                .as_ref()
                .is_some_and(|candidate| same_monitor(&monitor, candidate));
            let current = current
                .as_ref()
                .is_some_and(|candidate| same_monitor(&monitor, candidate));

            OverlayMonitorDescriptor {
                id: monitor_id(&monitor),
                name,
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
                scale_factor: monitor.scale_factor(),
                primary,
                current,
            }
        })
        .collect())
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
            info!("Démarrage du moteur Core de BakingRL...");

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

            // Configuration du mode "Click-Through" pour l'overlay
            if let Some(overlay_window) = _app.get_webview_window(INGAME_OVERLAY_LABEL) {
                info!("Activation du mode Click-Through pour l'overlay...");
                let _ = overlay_window.set_shadow(false);
                let _ = overlay_window.set_skip_taskbar(true);

                // Start hidden. Click-through is applied by the watcher after
                // the overlay has been shown at least once on Linux.
                let _ = overlay_window.hide();
            } else {
                warn!("Impossible de trouver la fenêtre 'overlay-ingame' pour activer le click-through.");
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
            plugin_host.apply_overlay_window_settings(&plugin_host.get_app_settings());
            _app.manage(plugin_host.clone());

            let obs_settings = plugin_host.get_app_settings().obs;
            let obs_addr = format!("{}:{}", obs_settings.host, obs_settings.port);
            match std::net::TcpListener::bind(&obs_addr) {
                Ok(listener) => {
                    let (_shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
                    crate::obs_gateway::start_obs_gateway(
                        bus.clone(),
                        plugin_host.clone(),
                        registry.clone(),
                        listener,
                        shutdown_rx,
                    );
                }
                Err(err) => warn!("Unable to start OBS gateway on {}: {}", obs_addr, err),
            }

            if plugin_host
                .get_app_settings()
                .behavior
                .start_minimized
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

            start_window_visibility_watcher(app_handle.clone(), plugin_host.clone());

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
            get_marketplace_catalog,
            refresh_marketplace,
            prepare_marketplace_package,
            install_prepared_package,
            discard_prepared_package,
            packages_dir,
            get_runtime_info,
            list_plugin_diagnostics,
            clear_plugin_diagnostics,
            get_app_settings,
            save_app_settings,
            get_package_settings,
            get_visual_settings_schema,
            save_package_settings,
            get_package_configuration_state,
            set_package_secret,
            delete_package_secret,
            set_package_enabled,
            remove_package,
            reload_packages,
            read_visual_export_source,
            call_service_export,
            plugin_registry_get,
            get_overlay_layouts,
            get_package_configuration_page,
            save_overlay_layout,
            create_overlay_layout,
            duplicate_overlay_layout,
            set_active_overlay_layout,
            set_stream_overlay_layout,
            delete_overlay_layout,
            import_package_layout,
            get_pages,
            save_page,
            create_page,
            duplicate_page,
            delete_page,
            import_package_page,
            open_page,
            open_package_configuration,
            open_package_secrets,
            registry_get,
            registry_entries,
            open_overlay_layout_editor,
            close_overlay_editor,
            get_telemetry_status,
            get_obs_gateway_status,
            list_overlay_monitors,
            emit_developer_telemetry,
            take_pending_package_file_opens,
        ])
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if window.label() == "main" {
                    let plugin_host = window.state::<Arc<PluginHost>>();
                    if plugin_host
                        .get_app_settings()
                        .behavior
                        .close_will_hide
                    {
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
