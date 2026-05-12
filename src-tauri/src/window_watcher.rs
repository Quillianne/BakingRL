use active_win_pos_rs::get_active_window;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tracing::{info, warn};

use crate::plugin_host::PluginHost;

fn apply_overlay_visibility(app_handle: &AppHandle, visible: bool) {
    let app_for_thread = app_handle.clone();
    if let Err(error) = app_handle.run_on_main_thread(move || {
        let Some(overlay_window) = app_for_thread.get_webview_window("overlay-ingame") else {
            return;
        };

        if visible {
            let _ = overlay_window.show();
            let _ = overlay_window.set_ignore_cursor_events(true);
            let _ = overlay_window.set_always_on_top(true);
        } else {
            let _ = overlay_window.hide();
        }
    }) {
        warn!("[WindowWatcher] Failed to schedule overlay visibility update: {error}");
    }
}

pub fn start_window_visibility_watcher(app_handle: AppHandle, plugin_host: Arc<PluginHost>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1000));
        let mut is_overlay_forced_visible = false;
        let mut overlay_visible: Option<bool> = None;

        loop {
            interval.tick().await;

            if app_handle.get_webview_window("overlay-editor").is_some() {
                if overlay_visible != Some(false) {
                    info!("[WindowWatcher] Hiding overlay while layout editor is open");
                    apply_overlay_visibility(&app_handle, false);
                }
                is_overlay_forced_visible = false;
                overlay_visible = Some(false);
                continue;
            }

            let hide_when_unfocused = plugin_host
                .get_app_settings()
                .overlay
                .hide_when_game_unfocused;
            if !hide_when_unfocused {
                if !is_overlay_forced_visible {
                    info!("[WindowWatcher] Showing overlay because focus hiding is disabled");
                    apply_overlay_visibility(&app_handle, true);
                    is_overlay_forced_visible = true;
                    overlay_visible = Some(true);
                }
                continue;
            }
            is_overlay_forced_visible = false;

            let is_game_focused = match get_active_window() {
                Ok(window) => {
                    let app_name = window.app_name.to_lowercase();
                    let title = window.title.to_lowercase();

                    app_name.contains("rocket league")
                        || app_name.contains("rocketleague")
                        || title.contains("rocket league")
                        || title.contains("rocketleague")
                }
                Err(_) => false,
            };

            if is_game_focused {
                if overlay_visible != Some(true) {
                    info!("[WindowWatcher] Showing overlay");
                    apply_overlay_visibility(&app_handle, true);
                }
                overlay_visible = Some(true);
            } else {
                if overlay_visible != Some(false) {
                    info!("[WindowWatcher] Hiding overlay");
                    apply_overlay_visibility(&app_handle, false);
                }
                overlay_visible = Some(false);
            }
        }
    });
}
