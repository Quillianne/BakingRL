pub mod bus;
pub mod ingestor;
pub mod models;
pub mod obs_gateway;
pub mod plugin_host;
pub mod plugin_v2;
pub mod registry;
pub mod window_watcher;

use crate::bus::{BusEvent, EventBus};
use crate::ingestor::{start_tcp_ingestor, TelemetryStatusState};
use crate::models::{GameEvent, TelemetryConnectionStatus};
use crate::plugin_host::{
    call_service_export, create_overlay_layout, create_page, delete_overlay_layout, delete_page,
    discard_prepared_package, duplicate_overlay_layout, duplicate_page, get_app_settings,
    get_overlay_layouts, get_package_settings, get_pages, get_visual_settings_schema,
    import_package_layout, import_package_page, inspect_package_bundle, install_package_from_file,
    install_package_from_url, install_prepared_package, list_packages, open_page, packages_dir,
    plugin_registry_get, prepare_package_from_deep_link, prepare_package_from_git,
    prepare_package_from_url, read_component_export_source, read_visual_export_source,
    reload_packages, remove_package, save_app_settings, save_overlay_layout, save_package_settings,
    save_page, set_active_overlay_layout, set_package_enabled, set_stream_overlay_layout,
    PluginHost,
};
use crate::registry::{registry_entries, registry_get, Registry};
use crate::window_watcher::start_window_visibility_watcher;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{
    Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

const INGAME_OVERLAY_LABEL: &str = "overlay-ingame";
const EDITOR_OVERLAY_LABEL: &str = "overlay-editor";

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Erreur lors de l'initialisation de tracing");

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(main_window) = app.get_webview_window("main") {
                let _ = main_window.show();
                let _ = main_window.set_focus();
            }
        }));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .setup(|_app| {
            info!("Démarrage du moteur Core de BakingRL...");

            let app_handle = _app.handle().clone();

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
                let _ = overlay_window.set_ignore_cursor_events(true);
                let _ = overlay_window.set_shadow(false);

                // Start with the window hidden, the watcher will show it if RL is in focus
                let _ = overlay_window.hide();
            } else {
                warn!("Impossible de trouver la fenêtre 'overlay-ingame' pour activer le click-through.");
            }

            let bus = Arc::new(EventBus::new(1024));
            let registry = Arc::new(Registry::new());
            _app.manage(bus.clone());
            _app.manage(registry.clone());

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
            tauri::async_runtime::spawn(async move {
                start_tcp_ingestor(
                    ingestor_bus,
                    ingestor_app_handle,
                    telemetry_status,
                    telemetry.rocket_league_host,
                    telemetry.rocket_league_port,
                )
                .await;
            });

            start_window_visibility_watcher(app_handle.clone(), plugin_host.clone());

            let mut rx = bus.subscribe();
            let telemetry_settings = plugin_host.clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                let mut last_update_state_emit: Option<Instant> = None;
                while let Ok(event) = rx.recv().await {
                    if let BusEvent::GameData(data) = event {
                        if data.event == "UpdateState" {
                            let fps = telemetry_settings
                                .get_app_settings()
                                .overlay
                                .update_rate_fps
                                .max(1);
                            let update_state_interval =
                                Duration::from_millis((1000 / u64::from(fps)).max(1));
                            let now = Instant::now();
                            if last_update_state_emit
                                .is_some_and(|last| now.duration_since(last) < update_state_interval)
                            {
                                continue;
                            }
                            last_update_state_emit = Some(now);
                        }

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
            get_app_settings,
            save_app_settings,
            get_package_settings,
            get_visual_settings_schema,
            save_package_settings,
            set_package_enabled,
            remove_package,
            reload_packages,
            read_visual_export_source,
            read_component_export_source,
            call_service_export,
            plugin_registry_get,
            get_overlay_layouts,
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
            registry_get,
            registry_entries,
            open_overlay_layout_editor,
            close_overlay_editor,
            get_telemetry_status,
            list_overlay_monitors,
            emit_developer_telemetry,
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
        .run(tauri::generate_context!())
        .expect("Erreur lors de l'exécution de l'application Tauri");
}
